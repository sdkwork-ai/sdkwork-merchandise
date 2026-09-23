-- ============================================================================
-- commerce_currency seed: the single authority for money scale and rounding.
-- ============================================================================
-- DATABASE_SPEC section 14 / DB095: the minor-unit exponent of a currency is
-- data, not code. Every money column in this module resolves its divisor from
-- this table (or from the `price_scale` snapshotted at write time), so no
-- layer may hardcode `/ 100`.
--
-- codes and exponents come from ISO 4217. `POINTS` is an SDKWork-internal
-- pricing unit with 6 fractional digits (1 point = 1e6 micro), matching
-- SUBJECT_ID_SPEC / account's CommercePrecision.POINTS_SCALE.
--
-- Application is pre-launch: this seed is the baseline reference data.
-- ============================================================================

INSERT INTO commerce_currency
    (id, code, minor_unit_exponent, rounding_mode, display_symbol, display_name, status, sort_order)
VALUES
    (1, 'CNY',    2, 'half_up',   '¥',  '人民币',        'active', 10),
    (2, 'USD',    2, 'half_up',   '$',  'US Dollar',     'active', 20),
    (3, 'EUR',    2, 'half_up',   '€',  'Euro',          'active', 30),
    (4, 'GBP',    2, 'half_up',   '£',  'Pound Sterling','active', 40),
    (5, 'HKD',    2, 'half_up',   'HK$','Hong Kong Dollar','active', 50),
    (6, 'JPY',    0, 'floor',     '¥',  'Japanese Yen',  'active', 60),
    (7, 'KRW',    0, 'floor',     '₩',  'South Korean Won','active', 70),
    (8, 'KWD',    3, 'half_up',   'KD', 'Kuwaiti Dinar', 'active', 80),
    (9, 'POINTS', 6, 'half_even', 'pt', 'SDKWork points','active', 900)
ON CONFLICT (id) DO UPDATE SET
    code = EXCLUDED.code,
    minor_unit_exponent = EXCLUDED.minor_unit_exponent,
    rounding_mode = EXCLUDED.rounding_mode,
    display_symbol = EXCLUDED.display_symbol,
    display_name = EXCLUDED.display_name,
    status = EXCLUDED.status,
    sort_order = EXCLUDED.sort_order,
    updated_at = NOW();
