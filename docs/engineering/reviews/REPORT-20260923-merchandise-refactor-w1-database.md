# Merchandise 商品体系重构 · 第 1 波（数据库与货币内核）落地报告

- 日期：2026-09-23
- 范围：`D:\sdkwork-space\sdkwork-merchandise`
- 上游依据：[REVIEW-20260923-merchandise-catalog-model-audit.md](REVIEW-20260923-merchandise-catalog-model-audit.md)（审计）、
  [PLAN-20260923-merchandise-catalog-model-v2.md](../plans/PLAN-20260923-merchandise-catalog-model-v2.md)（方案）
- 前置前提：应用尚未上线，允许完整对齐行业标准（用户 2026-09-23 明示）

---

## 0. 结论先行

本波交付**两件地基**：一个可复用的多精度货币内核，和一份按行业标准重写并通过真实数据库行为验证的基线 DDL。

| # | 交付物 | 状态 | 证据 |
| --- | --- | --- | --- |
| 1 | `sdkwork-commerce-money` 多精度货币 crate | ✅ 完成 | 19 单测全绿；`clippy -- -D warnings` 零告警；`cargo fmt --check` 干净 |
| 2 | 数据库基线重写（17 表） | ✅ 完成 | 87 条行为断言全绿；反向对照 7 通过 / 77 失败 |
| 3 | 种子重写（货币 / 目录 / 双语） | ✅ 完成 | 端到端 bootstrap + 幂等复跑行数不变 |
| 4 | 上线前迁移债清除 | ✅ 完成 | 官方工具判定 `already initialized` |
| 5 | 数据库契约重生成 | ✅ 完成 | 官方工具生成 17 表，未手改 |
| 6 | 可复用验证脚本 | ✅ 完成 | `pnpm db:verify:baseline` |
| 7 | Rust 仓储/服务/web-support 层对齐 | ❌ 未开始 | **当前与新基线不一致，见 §5** |
| 8 | OpenAPI 契约与能力令牌 | ❌ 未开始 | 见 §5 |
| 9 | 跨仓 `sdkwork-order` 金额单位 | ❌ 未开始（且本波使其更明确地暴露） | 见 §5.1 |

> **重要**：第 7 项未做，意味着**本仓 Rust 层目前与新基线不兼容**。这是有意的顺序选择（先钉死数据契约，再改代码），但必须显式记录，不能当成"已完成"。

---

## 1. 为什么必须先动金额单位（本波的起点）

用户明确纠正：种子里的 `640` 是 **640 元（主单位）**，不是 640 分。顺着这条线索查下去，暴露出的是**单位语义在整个链路上没有唯一真相**：

| 位置 | 现状 | 问题 |
| --- | --- | --- |
| `commerce_product_sku.price_amount TEXT` | 存 `'640'`，语义=元，但列上无货币、无精度声明 | DB052 / DB094 / DB096 违规 |
| `sdkwork-order` `format_money_minor` / `minor_units_to_major_decimal` | 硬编码 `/100` 与 `{:02}` | DB095 明令禁止的"硬编码除数" |
| `sdkwork-order` `decimal_sql_match_keys` | 靠"字符串里有没有小数点"猜是主单位还是最小单位 | 无声明，纯启发式 |
| `sdkwork-order` 充值查询 | 用 `price_amount IN ($4,$5,$6)` 三种小数串变体匹配 SKU | **单位不一致时会买错 SKU** |
| `sdkwork-account` `CommerceMoney(String)` | 裸 `String` newtype，无货币、无精度、无算术 | 根因：类型层面无法表达"金额" |
| 商品 OpenAPI | `CommerceOperationCommand` 仅 `additionalProperties: true` | 契约根本没有声明单位 |

DB098 要求"存储单位与 `API_SPEC` §13.2 暴露单位永不冲突"。所以要修的不是一个 `/100`，而是**让单位成为有类型、有声明、可校验的东西**。

---

## 2. 交付物 1：多精度货币内核 `sdkwork-commerce-money`

新增 crate（996 行，纯领域、零依赖、可 `Copy`）：

```
crates/sdkwork-commerce-money/
  Cargo.toml        lib 名 sdkwork_commerce_money；继承 workspace 的 version/edition/rust-version/lints
  src/lib.rs        MoneyUnit / UnitCode / Money / Rate / RoundingMode / MoneyError + 19 个单测
```

核心语义与设计取舍：

- **精度来自单位，不是来自代码。** `MoneyUnit { code, scale, rounding }`，`BUILT_IN_UNITS` 按 ISO 4217 给出 CNY/USD/EUR/GBP/HKD/MOP/TWD=2、JPY/KRW/VND=0、KWD/BHD/OMR/JOD/TND=3、CLF=4、POINTS/CREDIT=6。
  - 无零小数位货币下 `/100` 必然错：本 crate 用一个测试把这一点钉死（同样的数字 `640` 在 CNY 是 `640.00`、在 JPY 是 `640`）。
- **整数最小单位存储，写入不四舍五入**（DB099）。超过声明精度的输入在解析边界就**拒绝**（`ScaleExceeded`），除非调用方显式给 `parse_rounded(..., mode)`。
- **六种舍入模式**，方向敏感（Floor/Ceiling/Truncate/HalfUp/HalfDown/HalfEven）在**负数**上行为正确。
- **混单位算术被拒绝**而不是被重新解释：`checked_add` 返回 `UnitMismatch`，`PartialOrd` 返回 `None`（"不可比"比"编个序"诚实）。
- **分摊保总额** `allocate`：最大余额法 + 确定性 tiebreak（余数降序 → 权重降序 → 索引升序），负金额同样保总额（`-100.00` 按 `[3,1]` → `-75.00` / `-25.00`）。
- **换算只舍入一次** `convert_to`：一次精确除法后在目标精度上舍入，避免中间最小单位丢失。

### 2.1 我在本波中推翻自己的两处

1. **负数舍入的符号 bug（编译前靠推理发现，随后被测试证实）**
   超精度分支里我对**无符号量值**调用了 `div_round`，导致方向敏感模式在负数上是错的。第一次修正引入了新错误：把已带符号的商又取了一次负号（双重取负 → 负数变正数）。测试直接抓到：

   ```
   FAILED tests::negative_amounts_round_away_from_zero_for_half_up
     left: "-640.01"   right: "640.01"
   ```

   最终改为"两个分支都直接产出最终带符号值，结构体原样存储"，符号恰好应用一次。

2. **一条单元测试边界写错**
   `UnitCode::new("TOOLONGX")` 我断言应被拒 —— 但 `TOOLONGX` 正好 8 字符，而上限是 8，**它是合法的**；错的是我的断言。已改为用 9 字符验证上限，并补 3/8 两个边界点。
   同时发现文档与实现不符（注释写 "normalizes to upper case"，实现其实是**拒绝**小写而非归一化），已同步改正文档、说明这是为保持 `UnitCode: Copy` 且零分配的有意取舍。

> 一处**自造的过度约束**也被主动移除：我最初写了
> `CHECK (attribute_role = 'sales' OR NOT is_required OR attribute_role = 'key')`，
> 等于禁止 `parameter` 属性为必填。但"家具类目必须填材质"是合法业务，这条会挡住真实需求 —— 属自造技术债，已删。

---

## 3. 交付物 2：数据库基线重写

`database/ddl/baseline/postgres/0001_merchandise_baseline.sql`（871 行，17 表）。

### 3.1 结构指标（真实执行 `pg_constraint` / `pg_indexes` 统计）

| 指标 | 旧基线 | 新基线 |
| --- | --- | --- |
| 表 | 6 | **17** |
| CHECK 约束 | 0 | **74** |
| 外键 | 0 | **25** |
| 唯一约束 | 1 | **13** |
| 主键 | 6 | 17 |
| 索引 | 10 | **81**（其中 32 个部分索引） |
| `id` 列类型 | 6 个 TEXT | 17 个 **BIGINT** |
| 时间戳 | 16 个 TEXT | 49 个 **TIMESTAMPTZ** |
| 金额 | `*_amount TEXT`（主单位字符串） | 4 个 `*_minor BIGINT` + `price_scale` + `currency_code` |
| JSON | `spec_json TEXT` 承载 SKU 核心身份 | **0** 个 TEXT JSON；仅 1 个 `jsonb` 快照 |

### 3.2 表清单（17）

| 表 | 作用 |
| --- | --- |
| `commerce_currency` | 货币注册表：`minor_unit_exponent` + `rounding_mode` 的唯一权威 |
| `commerce_product_category` | 类目（前/后台双树：`parent_id` / `back_parent_id` + 物化路径 `path`） |
| `commerce_product_category_translation` | 类目本地化 |
| `commerce_product_attribute` | 属性字典（**不带 role**） |
| `commerce_product_attribute_translation` | 属性本地化 |
| `commerce_product_attribute_value` | 枚举值（含 `color_hex`、`media_resource_id` 色卡） |
| `commerce_product_attribute_value_translation` | 枚举值本地化 |
| `commerce_product_category_attribute` | **属性角色就在这里决定**（`key`/`sales`/`parameter`），模板物化下沉子类目 |
| `commerce_product_spu` | 商品身份（固定语义字段是列，不是 EAV） |
| `commerce_product_spu_translation` | SPU 本地化 |
| `commerce_product_spu_attribute` | SPU 的 key/parameter 值（替代 `spec_json`） |
| `commerce_product_sku` | 可售单元，含 `variant_signature` + 分层价格（最小单位） |
| `commerce_product_sku_attribute` | SKU 的销售轴取值 |
| `commerce_product_sku_translation` | SKU 本地化 |
| `commerce_product_media` | 媒体（引用 Drive 的稳定 `media_resource_id`，无裸 `url`） |
| `commerce_price_list` | 价格表头 |
| `commerce_price_list_item` | 价格表行（市场 × 客群 × 起订量 × 时间窗） |

### 3.3 四个关键设计决策

1. **属性角色属于"类目绑定"，不属于属性本身。**
   同一个 `material` 在类目 100 是 `key`、在类目 101 是 `parameter` —— 这不是巧合，而是被验证脚本显式断言的（`role is per-category`）。旧表的 `scope TEXT DEFAULT 'product'` 与代码里硬编码的 `'enum','product'` 因此整列消失。

2. **`variant_signature` 让 SPU/SKU 模型真正成立。**
   由服务端按"销售轴 `attribute_no=value_code`，按 `attribute_no` 升序，`;` 连接"确定性生成，`uk_commerce_product_sku_variant (tenant_id, spu_id, variant_signature) WHERE deleted_at IS NULL` 强制。验证脚本会**从数据库反算**签名与种子写入值比对，确保它不是装饰。

3. **两种价格层，拒绝往 SKU 上加列。**
   SKU 承载基础价（`list`/`sale`/`cost`），价格表承载市场/客群/阶梯/时间窗覆盖。`member_price` / `activity_price` 这类列一律不加。
   一致性由**复合外键**保证：`commerce_price_list_item(price_list_id, currency_code) → commerce_price_list(id, currency_code)`，价格表行**不可能**与其表头货币不一致（已断言）。

4. **本地化：基表承载默认语言，翻译表承载其余语言。**
   旧的双语种子都往 `commerce_product_sku.name` 写，**后跑的覆盖先跑的**（`en-US` 赢），根本无法共存；且 `spec_json.tags` 里塞的正是本地化促销文案。现在 zh-CN 在基表（也是 fallback），en-US 在翻译表，已验证两种语言同时存在。

   一并记录为**有意推迟**：促销角标/标签（原 `spec_json.tags`）属于促销能力的展示数据而非目录主数据，本次**不建半成品模型、也不塞回 JSON**，作为显式未完成项列出（见 §5.4）。

### 3.4 为兼容 `sdkwork-order` 而新增的一列（附说明）

`sdkwork-order` 的充值查询里有 `pr.sales_status = 'active'`，而旧 `commerce_product_spu` **根本没有 `sales_status` 列** —— 该查询运行即报"列不存在"。新基线给 SPU 补上了 `sales_status`（生命周期 `status` 之外的上下架状态，本身是合理概念）。但这只是把对方的查询变成"能跑"；§5.1 说明为什么最终必须改为读明确的价格列。

---

## 4. 验证方法与双向证据

`tests/verify_merchandise_baseline.py`（`pnpm db:verify:baseline`）对**真实 PostgreSQL 18.3** 执行两阶段验证，`autocommit` 逐条执行以避免一条预期失败回滚掉前面所有夹具。

- **第 1 阶段**：DDL 灌入一次性 schema，逐条断言每个不变量的**精确 SQLSTATE 与约束名**（不是"反正报错了"）。
- **第 2 阶段**：按 `seed.manifest.json` 顺序（并按其真实路径解析规则）应用基线 + 5 个种子文件，断言行数、**种子二次执行行数不变**、以及每个 `variant_signature` 可由销售轴行反算。

### 4.1 双向证据（这是"绿"有意义的前提）

| 对象 | 结果 |
| --- | --- |
| 新基线 + 新种子 | **87 passed, 0 failed** |
| 旧基线（`git show HEAD:...` 反向对照） | **7 passed, 77 failed** |

反向对照正是反假绿的证据。旧基线被抓到的问题包括：16 个 TEXT 时间戳、2 个 `*_amount` 主单位金额列、6 个非 BIGINT 主键、SPU 缺 `sales_status`。

### 4.2 覆盖到的关键不变量（节选）

- 640 元往返：`(CNY, scale 2, 64000)`，且 `sale == 640 * 10**scale` 由货币注册表的 exponent 反算确认
- 同 SPU 下 `variant_signature` 重复 → `23505` 精确命中 `uk_commerce_product_sku_variant`
- 引用价低于售价、精度越界、负价 → 各自 `23514` 命中对应 `ck_`
- 软删除后 `sku_no` 与签名被释放、可被新 SKU 复用（部分唯一索引）
- 价格表行货币与表头不一致 → `23503` 命中**复合外键**（用不同 `min_quantity` 隔离，避免被唯一约束抢先）
- 一个 owner 只能有一张主图；`y` 轴不能有两个取值；`spu_attribute` 两种载体只能有其一
- `organization_id` 不可空（哨兵 `0` 是唯一表示）

### 4.3 验证脚本自身被修掉的两个缺陷（否则会给出误导性结论）

1. **事务语义**：最初把断言包在单事务里，每条预期失败都会回滚全部前置夹具 → 一次真实缺陷放大成 30 条级联失败。改为逐条 `autocommit`。
2. **种子路径解析**：我按"裸名相对 seeds 根"实现，结果对 `001_bootstrap.sql` 报"文件不存在"。查权威实现 `sdkwork_database_spi::seed_manifest::resolve_common_script_path` 后确认规则是**裸名回退到 `common/`**、带 `common` 段才相对 seeds 根 —— 是**我的脚本错了，清单是对的**。已按实现逐字对齐（含 locale 规则）。

---

## 5. 未完成项（显式列出，不留隐性债）

### 5.1 🔴 破坏性跨仓影响：`sdkwork-order` 的充值查询必须同步改

本波**删除了 `price_amount` / `original_price_amount` 列**。`sdkwork-order` 的 `LOAD_RECHARGE_PRODUCT_SKU_FOR_AMOUNT` 仍然写：

```sql
SELECT CAST(s.id AS TEXT) AS sku_id, ...
FROM commerce_product_sku s JOIN commerce_product_spu pr ON pr.id = s.spu_id
...
ORDER BY CASE WHEN CAST(s.price_amount AS TEXT) IN ($4, $5, $6) THEN 0 ELSE 1 END, ...
```

**它现在会以"列不存在"直接失败**，且该查询还包含：
- `AND (s.organization_id = '0' OR s.organization_id = '0')` —— 条件重复
- `__PLATFORM_TENANT__` 字面量 —— 疑似未替换的模板
- 用内部 `id` 当跨服务身份（`CAST(s.id AS TEXT)`）

正确终态：改用稳定业务键 `sku_no` + 明确金额列 `sale_price_minor` / `currency_code`，并按 `commerce_currency.minor_unit_exponent` 呈现，不再猜单位、不再用三种小数串变体匹配。这是一个**小改动但必须做**，属第 2 波。

### 5.2 本仓 Rust 层与新基线不一致

| 位置 | 需要改什么 |
| --- | --- |
| `crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog.rs`（1411 行） | 全部 SQL 按新列名/类型重写；去掉内联 `uuid_v7()` / `now_iso8601()`（改用平台 ID 提供方与数据库时间）；金额读写改用 `Money` |
| 同上 `update_category` | `SET` 未包含 `path` / `level_no`，改名/改父后路径不更新；`build_category_path` 在异常时静默降级为根 —— 两者都要修（含环检测与删除守卫） |
| 同上（跨边界泄漏） | 读写 `commerce_cart` / `commerce_cart_item` / `commerce_user_address`（owner 是 `sdkwork-catalog`），但本仓路由清单里 **0 条** 购物车/地址路由 ⇒ 死代码 + 越界。须删除 |
| `crates/sdkwork-merchandise-web-support/src/catalog_store.rs` | `category_attributes` 4 个端点直接返回存储错误；`retrieve_sku_prices` 未实现 |
| `crates/sdkwork-merchandise-service/src/domain/mod.rs` | `ProductStatus` 枚举只有 draft/active/inactive/archived，代码却持久化 `'deleted'` —— 应改用 `deleted_at` |
| `sdkwork-account` `CommerceMoney(String)` | 应被 `sdkwork-commerce-money` 的 `Money` 取代 |

### 5.3 OpenAPI 契约仍是空的

`apis/backend-api/merchandise/shop-backend-api.merchandise.openapi.json` 里唯一的业务 schema 是 `CommerceOperationCommand` 且 `additionalProperties: true` —— 商品/类目/SKU **没有任何类型化 schema**，单位、精度、`int64 as string`（API_SPEC §13.6）都无从声明。另：服务契约令牌是 `commerce.catalog`、路由全在 `/backend/v3/api/catalog/*` —— 与 `DOMAIN_SPEC` 的"`catalog` 与 `merchandise` 是兄弟能力、不可互相替代"冲突。

### 5.4 有意推迟

- **促销角标/标签**：需要独立模型（含生效时间窗与本地化），不能塞 JSON、也不能只允许一条。属促销能力，非目录主数据。
- **门禁加固 + 变异验证**：审计 §6 提出的断言尚未落到门禁；`check-database-framework-standard.mjs` 在本仓返回 `passed` 的同时旧基线也是 `passed`（结构上偏弱），真正的证据是本报告的 87 条行为断言。新增门禁必须配变异自证。

### 5.5 需要你决策的跨仓配置改动

工作区权威 `sdkwork-specs/tools/database-module-registry.json` 有 **33 个模块，但 `sdkwork-merchandise`、`sdkwork-inventory`、`sdkwork-catalog`、`sdkwork-order`、`sdkwork-payment`、`sdkwork-shop` 全部不在其中** —— 整个 commerce 家族的前缀在权威注册表里**未登记**，这正是各仓自造 owner（如并不存在的 `sdkwork-commerce-platform`）的原因。

这是 DATABASE_SPEC §7 的硬要求（"`MUST` be registered in `tools/database-module-registry.json` before first use，且该条目是归属的唯一权威"）。但该注册表格式是"每模块一个 `tablePrefix`"，**无法表达 `commerce_` 的表族级归属**。两条路：

- **A**：给注册表增加表族级归属字段（影响全部 33 个模块的格式）
- **B**：为 commerce 家族改前缀（`merchandise_` / `inventory_` …），代价是所有硬编码 `commerce_*` 的跨仓 SQL 一起改

按你"配置改动先手动介入"的偏好，我没有擅自改这个共享仓。本波只把本仓 `database/contract/prefix-registry.json` 的 owner 从虚构的 `sdkwork-commerce-platform` 改成真实 owner `merchandise-platform`（由官方生成器写入）。

---

## 6. 回滚

本波所有改动都在 git 工作树内、未提交，可直接：

```bash
cd D:/sdkwork-space/sdkwork-merchandise
git checkout -- database/ package.json Cargo.toml Cargo.lock
rm -rf crates/sdkwork-commerce-money tests/verify_merchandise_baseline.py .workbuddy/tmp
```

注意 `Cargo.lock` 会因新增成员而变化；回滚后需重跑 `cargo metadata` 确认。

---

## 7. 复现命令

```bash
# 多精度货币内核
cargo test -p sdkwork-commerce-money
cargo clippy -p sdkwork-commerce-money --all-targets -- -D warnings

# 基线 + 种子行为验证（需可达的 PostgreSQL 与 psycopg）
pnpm db:verify:baseline

# 从基线重生成数据库契约
pnpm db:materialize:contract

# 反向对照（证明断言会红）
git show HEAD:database/ddl/baseline/postgres/0001_merchandise_baseline.sql > /tmp/old_baseline.sql
python tests/verify_merchandise_baseline.py /tmp/old_baseline.sql

# 框架门禁
pnpm db:validate
```
