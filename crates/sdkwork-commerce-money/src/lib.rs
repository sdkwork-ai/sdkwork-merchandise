//! Multi-precision commerce money.
//!
//! Amounts are stored as an **integer count of the unit's minor denomination** plus a
//! [`MoneyUnit`] that declares the unit's decimal scale. Nothing here is ever a float.
//!
//! The scale is a property of the unit, not of the value, so a value can never disagree
//! with its own unit: `640` in `CNY` (scale 2) is `6.40`, while `640` in `JPY` (scale 0)
//! is `640`. That is the whole point of the type — the previous `String` newtype carried
//! no unit, so nothing could tell those two apart.
//!
//! Rules enforced here:
//!
//! - Arithmetic between different units is a hard error, never a silent reinterpretation.
//! - Storage keeps exact minor units; rounding happens only when a value crosses the
//!   unit boundary (display, rate application, allocation remainder).
//! - [`Money::allocate`] and [`Money::split_evenly`] are **sum preserving**: the parts
//!   always add back up to the original, so discounts and taxes cannot leak a minor unit.
//! - Rounding mode is declared per unit rather than defaulted, so a stored value and a
//!   recomputed value agree (`DATABASE_SPEC` DB052, DB094–DB099).

use core::fmt;
use std::str::FromStr;

/// How an exact value is reduced to a unit's scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoundingMode {
    /// Away from zero on an exact half. The common commercial default.
    HalfUp,
    /// Toward zero on an exact half.
    HalfDown,
    /// To the even neighbour on an exact half (banker's rounding).
    HalfEven,
    /// Toward negative infinity.
    Floor,
    /// Toward positive infinity.
    Ceiling,
    /// Toward zero.
    Truncate,
}

impl RoundingMode {
    /// The persisted form of this mode.
    ///
    /// The vocabulary is pinned by the baseline CHECK constraint
    /// `ck_commerce_currency_rounding_mode`, so this mapping and that constraint must agree.
    #[must_use]
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::HalfUp => "half_up",
            Self::HalfDown => "half_down",
            Self::HalfEven => "half_even",
            Self::Floor => "floor",
            Self::Ceiling => "ceiling",
            Self::Truncate => "truncate",
        }
    }

    /// Parses a persisted `rounding_mode` value.
    ///
    /// Rejects anything the baseline would reject, so an unreadable registry row surfaces as a
    /// typed error at the boundary rather than as a silently defaulted rounding rule.
    pub fn from_storage_str(raw: &str) -> Result<Self, MoneyError> {
        let mode = match raw {
            "half_up" => Self::HalfUp,
            "half_down" => Self::HalfDown,
            "half_even" => Self::HalfEven,
            "floor" => Self::Floor,
            "ceiling" => Self::Ceiling,
            "truncate" => Self::Truncate,
            _ => return Err(MoneyError::UnknownRoundingMode),
        };
        Ok(mode)
    }
}

/// Rejection reasons for money construction and arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoneyError {
    /// An operation mixed two units. Convert explicitly first.
    UnitMismatch,
    /// The unit code is not in the registry and was not declared as a custom unit.
    UnknownUnit,
    /// The persisted rounding-mode text is not one the baseline CHECK constraint permits.
    UnknownRoundingMode,
    /// The unit code is not 3–8 ASCII alphanumerics.
    InvalidUnitCode,
    /// The requested scale exceeds the supported maximum.
    ScaleTooLarge,
    /// A 128-bit intermediate overflowed.
    Overflow,
    /// A divisor or weight total was zero.
    DivisionByZero,
    /// The literal is not a decimal amount.
    InvalidAmount,
    /// The literal carries more fractional digits than the unit allows.
    ScaleExceeded,
    /// A negative value was supplied where only zero or positive is meaningful.
    NegativeValue,
    /// Allocation received no weights.
    EmptyWeights,
    /// Allocation received a negative weight.
    NegativeWeight,
    /// Allocation weights summed to zero.
    ZeroWeightTotal,
}

impl fmt::Display for MoneyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnitMismatch => "money units do not match",
            Self::UnknownUnit => "money unit is not registered",
            Self::UnknownRoundingMode => "money rounding mode is not one the schema permits",
            Self::InvalidUnitCode => "money unit code must be 3 to 8 ASCII alphanumerics",
            Self::ScaleTooLarge => "money scale exceeds the supported maximum",
            Self::Overflow => "money arithmetic overflowed",
            Self::DivisionByZero => "money division by zero",
            Self::InvalidAmount => "money amount is not a decimal literal",
            Self::ScaleExceeded => "money amount has more fractional digits than the unit allows",
            Self::NegativeValue => "money value must not be negative",
            Self::EmptyWeights => "money allocation requires at least one weight",
            Self::NegativeWeight => "money allocation weights must not be negative",
            Self::ZeroWeightTotal => "money allocation weights must not sum to zero",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for MoneyError {}

/// Largest supported decimal scale. Covers ISO 4217 `CLF` (4) and internal point units (6)
/// with headroom for future units.
pub const MAX_SCALE: u8 = 8;

/// Longest accepted unit code. Mirrors the `commerce_currency_code_shape` CHECK constraint
/// (`code ~ '^[A-Z0-9]{3,8}$'`), so a code this type accepts is a code the database accepts.
pub const MAX_UNIT_CODE_LEN: usize = 8;

/// A 3–8 character upper-case ASCII alphanumeric unit identifier: an ISO 4217 alpha code such
/// as `CNY`, or an internal pricing unit such as `POINTS`.
///
/// The code lives in an inline fixed-size buffer rather than in a `&'static str`. Prices are
/// resolved from the `commerce_currency` registry row at request time, so a unit's code is
/// *data*, not a compile-time constant; a `&'static str` field forced every caller either to
/// match on a hand-written table of known currencies or to leak a string per request. The inline
/// buffer keeps `UnitCode` `Copy` and allocation-free while accepting a runtime code.
///
/// Lower case is still *rejected* rather than normalized: normalization would change the caller's
/// data silently, and the database CHECK rejects lower case too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnitCode {
    bytes: [u8; MAX_UNIT_CODE_LEN],
    len: u8,
}

impl UnitCode {
    pub const CNY: Self = Self::from_validated(b"CNY");
    pub const USD: Self = Self::from_validated(b"USD");
    pub const EUR: Self = Self::from_validated(b"EUR");
    pub const JPY: Self = Self::from_validated(b"JPY");
    pub const KWD: Self = Self::from_validated(b"KWD");
    pub const POINTS: Self = Self::from_validated(b"POINTS");

    /// Copies an already-validated code into the inline buffer.
    ///
    /// Private and const so the built-in constants can be declared without a `Result`. Every
    /// public entry point validates first; `MAX_UNIT_CODE_LEN` is the buffer length, and
    /// `new` rejects anything longer, so the copy cannot run past the end.
    const fn from_validated(raw: &[u8]) -> Self {
        let mut bytes = [0_u8; MAX_UNIT_CODE_LEN];
        let mut index = 0;
        while index < raw.len() {
            bytes[index] = raw[index];
            index += 1;
        }
        Self {
            bytes,
            len: raw.len() as u8,
        }
    }

    /// Validates a unit code: 3–8 upper-case ASCII alphanumerics.
    ///
    /// Accepts any `&str`, including one resolved from the database, because the code is data
    /// rather than a registry constant.
    pub fn new(raw: &str) -> Result<Self, MoneyError> {
        let value = raw.as_bytes();
        let length = value.len();
        let shaped = value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() && !byte.is_ascii_lowercase());
        if !(3..=MAX_UNIT_CODE_LEN).contains(&length) || !shaped {
            return Err(MoneyError::InvalidUnitCode);
        }
        Ok(Self::from_validated(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

impl fmt::Display for UnitCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A unit of value: its scale (fractional digits) and the rounding mode declared for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoneyUnit {
    code: UnitCode,
    scale: u8,
    rounding: RoundingMode,
}

impl MoneyUnit {
    /// Declares a custom unit. Used for units held in the database registry rather than
    /// the built-in ISO table.
    pub fn custom(code: UnitCode, scale: u8, rounding: RoundingMode) -> Result<Self, MoneyError> {
        if scale > MAX_SCALE {
            return Err(MoneyError::ScaleTooLarge);
        }
        Ok(Self {
            code,
            scale,
            rounding,
        })
    }

    /// Resolves a built-in ISO 4217 unit, or `POINTS` for the internal credit unit.
    pub fn of(code: UnitCode) -> Result<Self, MoneyError> {
        for (known, scale, rounding) in BUILT_IN_UNITS {
            if *known == code.as_str() {
                return Self::custom(code, *scale, *rounding);
            }
        }
        Err(MoneyError::UnknownUnit)
    }

    /// Builds a unit from a `commerce_currency` registry row.
    ///
    /// This is the entry point for every price that comes from the database. The scale is
    /// *read*, never assumed, because `DATABASE_SPEC` section 14 makes the minor-unit exponent
    /// data owned by that table; hardcoding a divisor (the historical `/ 100`) is the defect
    /// this path exists to make impossible.
    ///
    /// `rounding` is the registry row's `rounding_mode` text (`half_up`, `floor`, ...).
    pub fn from_registry(code: &str, scale: u8, rounding: &str) -> Result<Self, MoneyError> {
        let code = UnitCode::new(code)?;
        let rounding = RoundingMode::from_storage_str(rounding)?;
        Self::custom(code, scale, rounding)
    }

    #[must_use]
    pub const fn code(self) -> UnitCode {
        self.code
    }

    #[must_use]
    pub const fn scale(self) -> u8 {
        self.scale
    }

    #[must_use]
    pub const fn rounding(self) -> RoundingMode {
        self.rounding
    }

    /// The integer multiplier that converts a major amount at this scale into minor units.
    pub fn major_factor(self) -> Result<i128, MoneyError> {
        pow10(self.scale)
    }
}

/// Built-in unit registry: ISO 4217 currencies with a zero exponent, the four three-decimal
/// currencies, `CLF` at four, and the internal point unit at six.
const BUILT_IN_UNITS: &[(&str, u8, RoundingMode)] = &[
    ("CNY", 2, RoundingMode::HalfUp),
    ("USD", 2, RoundingMode::HalfUp),
    ("EUR", 2, RoundingMode::HalfUp),
    ("GBP", 2, RoundingMode::HalfUp),
    ("HKD", 2, RoundingMode::HalfUp),
    ("MOP", 2, RoundingMode::HalfUp),
    ("TWD", 2, RoundingMode::HalfUp),
    ("JPY", 0, RoundingMode::Floor),
    ("KRW", 0, RoundingMode::Floor),
    ("VND", 0, RoundingMode::Floor),
    ("KWD", 3, RoundingMode::HalfUp),
    ("BHD", 3, RoundingMode::HalfUp),
    ("OMR", 3, RoundingMode::HalfUp),
    ("JOD", 3, RoundingMode::HalfUp),
    ("TND", 3, RoundingMode::HalfUp),
    ("CLF", 4, RoundingMode::HalfEven),
    ("POINTS", 6, RoundingMode::HalfEven),
    ("CREDIT", 6, RoundingMode::HalfEven),
];

/// A decimal multiplier such as a tax rate, discount ratio, or exchange rate.
///
/// Kept as an exact scaled integer so rate application is deterministic and reproducible
/// on every node; `f64` is never used for money.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate {
    mantissa: i128,
    scale: u8,
}

impl Rate {
    pub fn new(mantissa: i128, scale: u8) -> Result<Self, MoneyError> {
        if scale > MAX_SCALE {
            return Err(MoneyError::ScaleTooLarge);
        }
        Ok(Self { mantissa, scale })
    }

    /// Identity multiplier (`1`).
    #[must_use]
    pub const fn one() -> Self {
        Self {
            mantissa: 1,
            scale: 0,
        }
    }

    #[must_use]
    pub const fn mantissa(self) -> i128 {
        self.mantissa
    }

    #[must_use]
    pub const fn scale(self) -> u8 {
        self.scale
    }
}

impl FromStr for Rate {
    type Err = MoneyError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (negative, digits) = match value.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, value.strip_prefix('+').unwrap_or(value)),
        };
        let (whole, fraction) = match digits.split_once('.') {
            Some((whole, fraction)) => (whole, fraction),
            None => (digits, ""),
        };
        if whole.is_empty() && fraction.is_empty() {
            return Err(MoneyError::InvalidAmount);
        }
        if !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(MoneyError::InvalidAmount);
        }
        let fraction_len = u8::try_from(fraction.len()).map_err(|_| MoneyError::ScaleTooLarge)?;
        if fraction_len > MAX_SCALE {
            return Err(MoneyError::ScaleTooLarge);
        }
        let mut mantissa: i128 = 0;
        for part in [whole, fraction] {
            for byte in part.bytes() {
                let digit = i128::from(byte - b'0');
                mantissa = mantissa
                    .checked_mul(10)
                    .and_then(|value| value.checked_add(digit))
                    .ok_or(MoneyError::Overflow)?;
            }
        }
        if negative {
            mantissa = -mantissa;
        }
        Ok(Self {
            mantissa,
            scale: fraction_len,
        })
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&scaled_to_decimal_string(self.mantissa, self.scale))
    }
}

/// An exact amount in a declared unit, held as an integer count of the unit's minor
/// denomination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    minor: i128,
    unit: MoneyUnit,
}

impl Money {
    /// Wraps an exact minor-unit count.
    #[must_use]
    pub const fn from_minor(minor: i128, unit: MoneyUnit) -> Self {
        Self { minor, unit }
    }

    /// Parses a major-denomination literal such as `"640.00"`.
    ///
    /// Rejects more fractional digits than the unit allows instead of rounding: a write
    /// path must not silently change an amount. Use [`Self::parse_rounded`] where rounding
    /// is the intended, declared behaviour.
    pub fn parse(amount: &str, unit: MoneyUnit) -> Result<Self, MoneyError> {
        Self::parse_with(amount, unit, None)
    }

    /// Parses a major-denomination literal, rounding excess fractional digits with `mode`.
    pub fn parse_rounded(
        amount: &str,
        unit: MoneyUnit,
        mode: RoundingMode,
    ) -> Result<Self, MoneyError> {
        Self::parse_with(amount, unit, Some(mode))
    }

    fn parse_with(
        amount: &str,
        unit: MoneyUnit,
        mode: Option<RoundingMode>,
    ) -> Result<Self, MoneyError> {
        let trimmed = amount.trim();
        let (negative, digits) = match trimmed.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
        };
        let (whole, fraction) = match digits.split_once('.') {
            Some((whole, fraction)) => (whole, fraction),
            None => (digits, ""),
        };
        if (whole.is_empty() && fraction.is_empty())
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(MoneyError::InvalidAmount);
        }

        let scale = usize::from(unit.scale);
        // Both branches yield the final signed minor amount, so the struct stores it
        // verbatim and the sign is applied exactly once.
        let minor = if fraction.len() <= scale {
            let mut value: i128 = 0;
            for part in [whole, fraction] {
                for byte in part.bytes() {
                    value = value
                        .checked_mul(10)
                        .and_then(|current| current.checked_add(i128::from(byte - b'0')))
                        .ok_or(MoneyError::Overflow)?;
                }
            }
            let padding =
                u32::try_from(scale - fraction.len()).map_err(|_| MoneyError::Overflow)?;
            let magnitude = value
                .checked_mul(pow10_u32(padding)?)
                .ok_or(MoneyError::Overflow)?;
            if negative {
                magnitude.checked_neg().ok_or(MoneyError::Overflow)?
            } else {
                magnitude
            }
        } else {
            let mode = mode.ok_or(MoneyError::ScaleExceeded)?;
            let keep = &fraction[..scale];
            let mut value: i128 = 0;
            for part in [whole, keep] {
                for byte in part.bytes() {
                    value = value
                        .checked_mul(10)
                        .and_then(|current| current.checked_add(i128::from(byte - b'0')))
                        .ok_or(MoneyError::Overflow)?;
                }
            }
            // Re-express the excess digits as the numerator of an exact division so the
            // declared rounding mode decides the last minor unit.
            let excess = &fraction[scale..];
            let mut drop_numerator: i128 = 0;
            for byte in excess.bytes() {
                drop_numerator = drop_numerator
                    .checked_mul(10)
                    .and_then(|current| current.checked_add(i128::from(byte - b'0')))
                    .ok_or(MoneyError::Overflow)?;
            }
            let drop_denominator =
                pow10_u32(u32::try_from(excess.len()).map_err(|_| MoneyError::Overflow)?)?;
            let magnitude = value
                .checked_mul(drop_denominator)
                .and_then(|current| current.checked_add(drop_numerator))
                .ok_or(MoneyError::Overflow)?;
            // Round the *signed* value so direction-sensitive modes (Floor, Ceiling,
            // Truncate) stay correct for negative amounts; the result is already signed.
            let signed = if negative {
                magnitude.checked_neg().ok_or(MoneyError::Overflow)?
            } else {
                magnitude
            };
            div_round(signed, drop_denominator, mode)?
        };

        Ok(Self { minor, unit })
    }

    #[must_use]
    pub const fn minor(self) -> i128 {
        self.minor
    }

    #[must_use]
    pub const fn unit(self) -> MoneyUnit {
        self.unit
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.minor == 0
    }

    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.minor < 0
    }

    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.minor > 0
    }

    /// Rejects a negative amount. Zero is accepted.
    pub fn require_non_negative(self) -> Result<Self, MoneyError> {
        if self.is_negative() {
            return Err(MoneyError::NegativeValue);
        }
        Ok(self)
    }

    /// Wraps a plain minor-unit integer literal in the given unit.
    pub fn from_minor_str(minor: &str, unit: MoneyUnit) -> Result<Self, MoneyError> {
        let trimmed = minor.trim();
        let value = trimmed
            .parse::<i128>()
            .map_err(|_| MoneyError::InvalidAmount)?;
        Ok(Self { minor: value, unit })
    }

    /// Exact addition. Different units are rejected.
    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        self.ensure_same_unit(other)?;
        Ok(Self {
            minor: self
                .minor
                .checked_add(other.minor)
                .ok_or(MoneyError::Overflow)?,
            unit: self.unit,
        })
    }

    /// Exact subtraction. Different units are rejected.
    pub fn checked_sub(self, other: Self) -> Result<Self, MoneyError> {
        self.ensure_same_unit(other)?;
        Ok(Self {
            minor: self
                .minor
                .checked_sub(other.minor)
                .ok_or(MoneyError::Overflow)?,
            unit: self.unit,
        })
    }

    /// Exact multiplication by a whole count of units (quantity, pack size, installments).
    pub fn mul_int(self, quantity: i64) -> Result<Self, MoneyError> {
        Ok(Self {
            minor: self
                .minor
                .checked_mul(i128::from(quantity))
                .ok_or(MoneyError::Overflow)?,
            unit: self.unit,
        })
    }

    /// Exact division by a whole count, rounding with the unit's declared mode.
    pub fn div_int(self, divisor: i64) -> Result<Self, MoneyError> {
        if divisor == 0 {
            return Err(MoneyError::DivisionByZero);
        }
        Ok(Self {
            minor: div_round(self.minor, i128::from(divisor), self.unit.rounding)?,
            unit: self.unit,
        })
    }

    /// Applies a decimal multiplier (tax, discount, markup) rounding to this unit's scale.
    pub fn apply_rate(self, rate: Rate) -> Result<Self, MoneyError> {
        let numerator = self
            .minor
            .checked_mul(rate.mantissa)
            .ok_or(MoneyError::Overflow)?;
        let denominator = pow10_u32(u32::from(rate.scale))?;
        Ok(Self {
            minor: div_round(numerator, denominator, self.unit.rounding)?,
            unit: self.unit,
        })
    }

    /// Converts into another unit using an exact rate expressed as *target per source*.
    ///
    /// The conversion is a single exact division so no intermediate minor unit is lost,
    /// and the result is rounded once at the target unit's declared scale.
    pub fn convert_to(self, target: MoneyUnit, rate: Rate) -> Result<Self, MoneyError> {
        let source_scale = u32::from(self.unit.scale);
        let target_scale = u32::from(target.scale);
        let numerator = self
            .minor
            .checked_mul(rate.mantissa)
            .and_then(|value| value.checked_mul(pow10_u32(target_scale).ok()?))
            .ok_or(MoneyError::Overflow)?;
        let denominator_scale = u32::from(rate.scale)
            .checked_add(source_scale)
            .ok_or(MoneyError::Overflow)?;
        let denominator = pow10_u32(denominator_scale)?;
        Ok(Self {
            minor: div_round(numerator, denominator, target.rounding)?,
            unit: target,
        })
    }

    /// Splits into proportional parts by integer weights using the largest-remainder
    /// method, so the parts always sum back to exactly this amount.
    pub fn allocate(self, weights: &[i64]) -> Result<Vec<Self>, MoneyError> {
        if weights.is_empty() {
            return Err(MoneyError::EmptyWeights);
        }
        let mut total_weight: i128 = 0;
        for weight in weights {
            if *weight < 0 {
                return Err(MoneyError::NegativeWeight);
            }
            total_weight = total_weight
                .checked_add(i128::from(*weight))
                .ok_or(MoneyError::Overflow)?;
        }
        if total_weight == 0 {
            return Err(MoneyError::ZeroWeightTotal);
        }

        let total = self.minor;
        let mut parts: Vec<i128> = Vec::with_capacity(weights.len());
        let mut remainders: Vec<(i128, usize)> = Vec::with_capacity(weights.len());
        let mut assigned: i128 = 0;
        for (index, weight) in weights.iter().enumerate() {
            let numerator = total
                .checked_mul(i128::from(*weight))
                .ok_or(MoneyError::Overflow)?;
            let base = numerator.div_euclid(total_weight);
            let remainder = numerator.rem_euclid(total_weight);
            assigned = assigned.checked_add(base).ok_or(MoneyError::Overflow)?;
            parts.push(base);
            remainders.push((remainder, index));
        }

        // Largest remainder first; ties go to the larger weight, then the lower index, so
        // the split is deterministic and reproducible across nodes and reruns.
        remainders.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| weights[right.1].cmp(&weights[left.1]))
                .then_with(|| left.1.cmp(&right.1))
        });

        let mut leftover = total.checked_sub(assigned).ok_or(MoneyError::Overflow)?;
        for (_, index) in remainders {
            if leftover == 0 {
                break;
            }
            parts[index] = parts[index].checked_add(1).ok_or(MoneyError::Overflow)?;
            leftover -= 1;
        }

        Ok(parts
            .into_iter()
            .map(|minor| Self {
                minor,
                unit: self.unit,
            })
            .collect())
    }

    /// Splits into `count` parts as evenly as possible, still sum preserving.
    pub fn split_evenly(self, count: usize) -> Result<Vec<Self>, MoneyError> {
        if count == 0 {
            return Err(MoneyError::DivisionByZero);
        }
        let weights = vec![1_i64; count];
        self.allocate(&weights)
    }

    /// Sums amounts that must all share one unit.
    pub fn sum(amounts: &[Self]) -> Result<Self, MoneyError> {
        let mut total: Option<Self> = None;
        for amount in amounts {
            total = Some(match total {
                Some(current) => current.checked_add(*amount)?,
                None => *amount,
            });
        }
        total.ok_or(MoneyError::EmptyWeights)
    }

    /// Major-denomination decimal string carrying exactly the unit's scale.
    #[must_use]
    pub fn to_major_string(&self) -> String {
        scaled_to_decimal_string(self.minor, self.unit.scale)
    }

    fn ensure_same_unit(self, other: Self) -> Result<(), MoneyError> {
        if self.unit.code != other.unit.code || self.unit.scale != other.unit.scale {
            return Err(MoneyError::UnitMismatch);
        }
        Ok(())
    }
}

impl fmt::Display for Money {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {}",
            scaled_to_decimal_string(self.minor, self.unit.scale),
            self.unit.code
        )
    }
}

/// Comparison is only meaningful inside one unit, so a cross-unit comparison reports
/// "unordered" rather than inventing an ordering.
impl PartialOrd for Money {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.unit.code != other.unit.code || self.unit.scale != other.unit.scale {
            return None;
        }
        Some(self.minor.cmp(&other.minor))
    }
}

/// Exact division with an explicit rounding mode.
///
/// `numerator` may be negative; `denominator` must be non-zero. The quotient is computed
/// with Euclidean division so the remainder is always non-negative, which makes every mode
/// expressible without sign special cases.
pub fn div_round(
    numerator: i128,
    denominator: i128,
    mode: RoundingMode,
) -> Result<i128, MoneyError> {
    if denominator == 0 {
        return Err(MoneyError::DivisionByZero);
    }
    let (numerator, denominator) = if denominator < 0 {
        (
            numerator.checked_neg().ok_or(MoneyError::Overflow)?,
            denominator.checked_neg().ok_or(MoneyError::Overflow)?,
        )
    } else {
        (numerator, denominator)
    };

    let floor = numerator.div_euclid(denominator);
    let remainder = numerator.rem_euclid(denominator);
    if remainder == 0 {
        return Ok(floor);
    }

    let doubled = remainder.checked_mul(2).ok_or(MoneyError::Overflow)?;
    let comparison = doubled.cmp(&denominator);
    let negative = numerator < 0;

    let round_up = match mode {
        RoundingMode::Floor => false,
        RoundingMode::Ceiling => true,
        RoundingMode::Truncate => negative,
        RoundingMode::HalfUp => match comparison {
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Less => false,
            core::cmp::Ordering::Equal => !negative,
        },
        RoundingMode::HalfDown => match comparison {
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Less => false,
            core::cmp::Ordering::Equal => negative,
        },
        RoundingMode::HalfEven => match comparison {
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Less => false,
            core::cmp::Ordering::Equal => floor % 2 != 0,
        },
    };

    if round_up {
        floor.checked_add(1).ok_or(MoneyError::Overflow)
    } else {
        Ok(floor)
    }
}

fn pow10_u32(exponent: u32) -> Result<i128, MoneyError> {
    let mut value: i128 = 1;
    for _ in 0..exponent {
        value = value.checked_mul(10).ok_or(MoneyError::Overflow)?;
    }
    Ok(value)
}

fn pow10(exponent: u8) -> Result<i128, MoneyError> {
    pow10_u32(u32::from(exponent))
}

fn scaled_to_decimal_string(minor: i128, scale: u8) -> String {
    let scale = usize::from(scale);
    let negative = minor < 0;
    let magnitude = minor.unsigned_abs().to_string();
    let digits = if magnitude.len() > scale {
        magnitude
    } else {
        let mut padded = String::with_capacity(scale + 1);
        for _ in 0..(scale + 1 - magnitude.len()) {
            padded.push('0');
        }
        padded.push_str(&magnitude);
        padded
    };

    let split = digits.len() - scale;
    let mut rendered = String::with_capacity(digits.len() + 2);
    if negative {
        rendered.push('-');
    }
    if scale == 0 {
        rendered.push_str(&digits);
    } else {
        rendered.push_str(&digits[..split]);
        rendered.push('.');
        rendered.push_str(&digits[split..]);
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::{div_round, Money, MoneyError, MoneyUnit, Rate, RoundingMode, UnitCode, MAX_SCALE};

    fn cny() -> MoneyUnit {
        MoneyUnit::of(UnitCode::CNY).expect("CNY is a built-in unit")
    }

    fn jpy() -> MoneyUnit {
        MoneyUnit::of(UnitCode::JPY).expect("JPY is a built-in unit")
    }

    #[test]
    fn scale_comes_from_the_unit_so_the_same_digits_mean_different_amounts() {
        let yuan = Money::from_minor_str("64000", cny()).expect("valid minor amount");
        let yen = Money::from_minor_str("640", jpy()).expect("valid minor amount");
        assert_eq!("640.00", yuan.to_major_string());
        assert_eq!("640", yen.to_major_string());
        assert_eq!("640.00 CNY", yuan.to_string());
        assert_eq!("640 JPY", yen.to_string());
    }

    #[test]
    fn parses_major_literals_and_rejects_excess_precision_instead_of_rounding() {
        let amount = Money::parse("640.00", cny()).expect("valid major amount");
        assert_eq!(64000, amount.minor());

        assert_eq!(
            Err(MoneyError::ScaleExceeded),
            Money::parse("640.001", cny())
        );
    }

    #[test]
    fn parses_and_rounds_at_every_declared_mode() {
        let cases = [
            (RoundingMode::HalfUp, "640.005", "640.01"),
            (RoundingMode::HalfDown, "640.005", "640.00"),
            (RoundingMode::HalfEven, "640.005", "640.00"),
            (RoundingMode::HalfEven, "640.015", "640.02"),
            (RoundingMode::Floor, "640.009", "640.00"),
            (RoundingMode::Ceiling, "640.001", "640.01"),
            (RoundingMode::Truncate, "640.009", "640.00"),
        ];
        for (mode, literal, expected) in cases {
            let amount =
                Money::parse_rounded(literal, cny(), mode).expect("rounding a literal succeeds");
            assert_eq!(
                expected,
                amount.to_major_string(),
                "{literal} under {mode:?}"
            );
        }
    }

    #[test]
    fn negative_amounts_round_away_from_zero_for_half_up() {
        let amount = Money::parse_rounded("-640.005", cny(), RoundingMode::HalfUp)
            .expect("rounding a negative literal succeeds");
        assert_eq!("-640.01", amount.to_major_string());
        assert!(amount.is_negative());
        assert_eq!(
            Err(MoneyError::NegativeValue),
            amount.require_non_negative()
        );
    }

    #[test]
    fn direction_sensitive_rounding_stays_correct_for_negative_amounts() {
        let cases = [
            (RoundingMode::Floor, "-640.001", "-640.01"),
            (RoundingMode::Ceiling, "-640.001", "-640.00"),
            (RoundingMode::Truncate, "-640.009", "-640.00"),
            (RoundingMode::HalfEven, "-640.005", "-640.00"),
            (RoundingMode::HalfEven, "-640.015", "-640.02"),
        ];
        for (mode, literal, expected) in cases {
            let amount =
                Money::parse_rounded(literal, cny(), mode).expect("rounding a literal succeeds");
            assert_eq!(
                expected,
                amount.to_major_string(),
                "{literal} under {mode:?}"
            );
        }
    }

    #[test]
    fn arithmetic_rejects_mixed_units_instead_of_reinterpreting_them() {
        let yuan = Money::parse("640.00", cny()).expect("valid");
        let yen = Money::parse("640", jpy()).expect("valid");
        assert_eq!(Err(MoneyError::UnitMismatch), yuan.checked_add(yen));
        assert_eq!(None, yuan.partial_cmp(&yen));
    }

    #[test]
    fn addition_and_quantity_multiplication_are_exact() {
        let unit_price = Money::parse("640.00", cny()).expect("valid");
        let total = unit_price
            .mul_int(3)
            .and_then(|value| value.checked_add(Money::parse("0.07", cny()).expect("valid")))
            .expect("exact arithmetic");
        assert_eq!("1920.07", total.to_major_string());
        assert_eq!(192007, total.minor());
    }

    #[test]
    fn allocation_is_sum_preserving_for_awkward_ratios() {
        let total = Money::parse("100.00", cny()).expect("valid");
        let parts = total
            .allocate(&[1, 1, 1])
            .expect("three-way split succeeds");
        let rendered: Vec<String> = parts.iter().map(Money::to_major_string).collect();
        assert_eq!(vec!["33.34", "33.33", "33.33"], rendered);
        assert_eq!(total, Money::sum(&parts).expect("sum succeeds"));
    }

    #[test]
    fn allocation_keeps_the_total_for_negative_amounts_too() {
        let total = Money::from_minor(-10000, cny());
        let parts = total.allocate(&[3, 1]).expect("split succeeds");
        assert_eq!(total, Money::sum(&parts).expect("sum succeeds"));
        let rendered: Vec<String> = parts.iter().map(Money::to_major_string).collect();
        assert_eq!(vec!["-75.00", "-25.00"], rendered);
    }

    #[test]
    fn allocation_rejects_degenerate_weight_sets() {
        let total = Money::parse("10.00", cny()).expect("valid");
        assert_eq!(Err(MoneyError::EmptyWeights), total.allocate(&[]));
        assert_eq!(Err(MoneyError::NegativeWeight), total.allocate(&[1, -1]));
        assert_eq!(Err(MoneyError::ZeroWeightTotal), total.allocate(&[0, 0]));
    }

    #[test]
    fn even_split_distributes_the_remainder_once() {
        let total = Money::parse("10.00", cny()).expect("valid");
        let parts = total.split_evenly(3).expect("split succeeds");
        let rendered: Vec<String> = parts.iter().map(Money::to_major_string).collect();
        assert_eq!(vec!["3.34", "3.33", "3.33"], rendered);
        assert_eq!(total, Money::sum(&parts).expect("sum succeeds"));
    }

    #[test]
    fn rate_application_is_deterministic_and_stays_exact_within_scale() {
        let price = Money::parse("640.00", cny()).expect("valid");
        let discounted = price
            .apply_rate("0.97".parse::<Rate>().expect("valid rate"))
            .expect("rate applies");
        assert_eq!("620.80", discounted.to_major_string());

        let tax = price
            .apply_rate("0.06".parse::<Rate>().expect("valid rate"))
            .expect("rate applies");
        assert_eq!("38.40", tax.to_major_string());
    }

    #[test]
    fn conversion_rounds_once_at_the_target_scale() {
        let yuan = Money::parse("640.00", cny()).expect("valid");
        let yen = yuan
            .convert_to(jpy(), "20.5".parse::<Rate>().expect("valid rate"))
            .expect("conversion succeeds");
        // 640.00 CNY * 20.5 = 13120 JPY exactly; scale 0 keeps it integral.
        assert_eq!("13120", yen.to_major_string());
        assert_eq!(13120, yen.minor());
    }

    #[test]
    fn zero_decimal_units_reject_fractional_input() {
        assert_eq!(Err(MoneyError::ScaleExceeded), Money::parse("100.5", jpy()));
        let rounded = Money::parse_rounded("100.5", jpy(), RoundingMode::HalfEven)
            .expect("rounding succeeds");
        assert_eq!("100", rounded.to_major_string());
    }

    #[test]
    fn division_rejects_zero_and_rounds_with_the_unit_mode() {
        let amount = Money::parse("10.00", cny()).expect("valid");
        assert_eq!(Err(MoneyError::DivisionByZero), amount.div_int(0));
        assert_eq!(
            "3.33",
            amount.div_int(3).expect("divides").to_major_string()
        );
    }

    #[test]
    fn custom_units_carry_their_own_scale_and_rounding() {
        let points = MoneyUnit::custom(
            UnitCode::new("POINTS").expect("valid code"),
            6,
            RoundingMode::HalfEven,
        )
        .expect("custom unit is valid");
        let amount = Money::from_minor(18_000_000, points);
        assert_eq!("18.000000", amount.to_major_string());
        assert_eq!(6, points.scale());
        assert_eq!(RoundingMode::HalfEven, points.rounding());
    }

    #[test]
    fn unit_codes_are_validated() {
        assert!(UnitCode::new("CNY").is_ok());
        // Boundary: 3 and 8 characters are the accepted extremes.
        assert!(UnitCode::new("ABC").is_ok());
        assert!(UnitCode::new("ABCDEFGH").is_ok());
        assert_eq!(Err(MoneyError::InvalidUnitCode), UnitCode::new("AB"));
        assert_eq!(Err(MoneyError::InvalidUnitCode), UnitCode::new("ABCDEFGHI"));
        assert_eq!(Err(MoneyError::InvalidUnitCode), UnitCode::new("cn"));
        assert_eq!(Err(MoneyError::InvalidUnitCode), UnitCode::new("CN"));
        assert_eq!(Err(MoneyError::InvalidUnitCode), UnitCode::new("cny"));
        assert_eq!(Err(MoneyError::InvalidUnitCode), UnitCode::new("CN-Y"));
        assert_eq!(
            Err(MoneyError::UnknownUnit),
            MoneyUnit::of(UnitCode::new("ZZZ").expect("well formed but unregistered"))
        );
    }

    #[test]
    fn runtime_currency_codes_resolve_from_the_registry_row() {
        // `KRW` is a real ISO code that is deliberately not one of the built-in constants:
        // exactly the situation the `commerce_currency` registry creates at request time.
        let unit = MoneyUnit::from_registry("KRW", 0, "floor").expect("registry row resolves");
        assert_eq!("KRW", unit.code().as_str());
        assert_eq!(0, unit.scale());
        assert_eq!(RoundingMode::Floor, unit.rounding());

        // The scale comes from the row, so one literal means different minor amounts per unit.
        // This is the property that makes a hardcoded `/ 100` wrong.
        assert_eq!(640, Money::parse("640", unit).expect("valid").minor());
        let cny = MoneyUnit::from_registry("CNY", 2, "half_up").expect("registry row resolves");
        assert_eq!(64_000, Money::parse("640", cny).expect("valid").minor());

        // Excess fractional digits are rejected, never rounded away behind the caller's back.
        assert_eq!(Err(MoneyError::ScaleExceeded), Money::parse("640.5", unit));
    }

    #[test]
    fn registry_vocabulary_is_validated_in_both_directions() {
        for mode in [
            RoundingMode::HalfUp,
            RoundingMode::HalfDown,
            RoundingMode::HalfEven,
            RoundingMode::Floor,
            RoundingMode::Ceiling,
            RoundingMode::Truncate,
        ] {
            assert_eq!(
                Ok(mode),
                RoundingMode::from_storage_str(mode.as_storage_str())
            );
        }
        assert_eq!(
            Err(MoneyError::UnknownRoundingMode),
            RoundingMode::from_storage_str("bankers")
        );
        assert_eq!(
            Err(MoneyError::InvalidUnitCode),
            MoneyUnit::from_registry("krw", 0, "floor")
        );
    }

    #[test]
    fn scale_ceiling_is_enforced() {
        assert_eq!(
            Err(MoneyError::ScaleTooLarge),
            MoneyUnit::custom(UnitCode::CNY, MAX_SCALE + 1, RoundingMode::HalfUp)
        );
    }

    #[test]
    fn division_reports_overflow_instead_of_wrapping() {
        assert_eq!(
            Err(MoneyError::Overflow),
            div_round(i128::MIN, -1, RoundingMode::HalfUp)
        );
    }
}
