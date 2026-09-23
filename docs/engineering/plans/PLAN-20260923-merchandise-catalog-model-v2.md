# PLAN-20260923 商品体系 v2 设计方案与优化

Status: proposed
Owner: SDKWork maintainers
Application: `sdkwork-merchandise`（跨 `sdkwork-inventory` / `sdkwork-catalog` / `sdkwork-order` / `sdkwork-cloudrouter`）
Scope: SPU / SKU / 属性体系（基础属性·交易属性）/ 价格 / 库存 / 类目 六者的模型设计与优化
Baseline commit: `6131b62e`
Predecessor: [REVIEW-20260923 商品与类目体系审计](../reviews/REVIEW-20260923-merchandise-catalog-model-audit.md)
Constraint: 本仓 `AGENTS.md` 规定无 database-owner 评审不得新增 schema/DDL/迁移 —— 本方案是**提案**，W0/W1 波刻意零 schema 变更

---

## 0. 结论先行

### 设计要点一览

| 对象 | 一句话设计主张 |
|---|---|
| **SPU** | 「这个商品是什么」——承载**基础属性**、类目陈列（多对多）、品牌、媒体。不含价格、不含库存。 |
| **SKU** | 「这个具体卖哪个版本」——由 **交易属性（销售属性）的取值组合**唯一确定，并带一个 `variant_signature` 去重。承载基准价、条码、重量、税类、交易约束。 |
| **基础属性** | 又称参数属性。`role = parameter`。挂 **SPU**，用于展示与筛选，**不参与 SKU 区分**。 |
| **交易属性** | 又称销售属性。`role = sales`。在 SPU 上**声明为轴**，在 SKU 上**取具体值**，直接决定 SKU 笛卡尔组合。 |
| **价格** | **两层**：SKU 基准价（list/sale/cost）+ 价目表明细（市场 × 客群 × 阶梯量 × 时间窗的覆盖价）。**不**在建表时堆 `member_price`/`activity_price` 列。 |
| **库存** | **不属于商品**。owner 是 `sdkwork-inventory`（已存在）。商品侧只声明 *是否追踪* + *扣减策略*，通过 `sku_id` 对接。 |
| **类目** | 树 + **属性模板**（类目绑定属性，可继承），新建商品时按类目拉出必填清单。 |

### 与现状的核心差距

| # | 现状 | 目标 | 影响 |
|---|---|---|---|
| 1 | 规格只塞 SKU 的 `spec_json` 自由 JSON | 交易属性独立成表，SKU 取值可查可索引 | 无法按颜色/尺码筛选、无法校验规格组合 |
| 2 | 只有一张扁平属性表，`value_type`/`scope` 被 SQL 硬编码 | `attribute_role` 三分（key/sales/parameter）+ 类型化 | 参数属性与销售属性混为一谈，SPU/SKU 模型不成立 |
| 3 | SPU **完全没有**属性落库面 | `commerce_product_spu_attribute` | 基础属性无处可存 |
| 4 | 价格只有 SKU 单值，价目表有头无明细 | + `commerce_price_list_item` | 会员价/阶梯价/区域价无法表达 |
| 5 | 库存计数 6 列互相派生，无唯一键、无流水 | + 唯一键 + append-only ledger + 派生列一致性约束 | 同 SKU 同仓多行 → 超卖/不可售；无法对账 |
| 6 | 8 个仓都注册 `commerce_` 前缀；merchandise 越界写 catalog 的 3 张表 | 注册精度下沉到表级前缀；移除越界写 | 所有权无法仲裁 |

---

## 1. 设计原则

1. **一律以「组合」而非「列」表达多变结构。** 规格、属性值、类目陈列、价格，全部走关系表；列只承载固定语义（名称、状态、金额、维度）。
2. **属性的角色由类目模板决定，属性自身只声明能力。** 同一「颜色」属性，在服装类目是销售属性、在家具类目可以是参数属性 —— 角色放在 `category_attribute` 上，允许覆盖。
3. **可售性是一张投影，不是一堆列。** `status`（生命周期）× `sales_status`（售卖）× 价格有效 × 库存充足 × 时间窗，由读模型（`sdkwork-catalog`）计算，商品侧不存派生布尔。
4. **金额只存 minor 整数 + 币种 + scale。** 禁止在业务代码里出现 `/100`。
5. **库存是别人的事实。** 商品侧对库存只有「声明」，没有「事实」。

---

## 2. 六者关系（概念模型）

```text
品牌 brand ──┐
             ├──> SPU ──────────┬── SPU 基础属性值 (role=parameter)
类目 category ─┘   │            ├── 媒体 media (owner_type=spu)
   │               │            ├── 类目陈列 (多对多, primary_flag)
   │               │            └── 翻译 translation
   │               │
   └─ 类目属性模板 category_attribute ──> 决定本类目「必填/可筛/是否轴」的属性清单
        │
        └───────────> SKU ─────┬── SKU 交易属性取值 (role=sales) ← 决定 SKU 身份
                     │         ├── 媒体 media (owner_type=sku)
                     │         ├── 基准价 list/sale/cost
                     │         ├── 交易约束 min/max/step/backorder
                     │         └── sku_id ────> [ sdkwork-inventory ]
                     │                              commerce_inventory_stock
                     │                              commerce_inventory_movement (ledger)
                     └────> 价目表明细 price_list_item (覆盖基准价)
```

---

## 3. 属性体系设计（本次的核心）

### 3.1 三类属性 + 系统字段

行业成熟做法（国内电商的「关键属性 / 销售属性 / 非关键属性」，对应 Magento 的
`super attribute` vs `custom attribute` + attribute set，Shopify 的 `option` vs `metafield`）
收敛为三类**角色**：

| 角色 | 别称 | 作用 | 值挂在哪层 | 参与 SKU 区分 |
|---|---|---|---|---|
| `key` | 关键属性 | 判定商品归属类目（如「是手机还是平板」） | SPU | 否 |
| `sales` | 销售属性 / **交易属性** | 决定 SKU 组合（如 颜色、尺码、容量、套餐时长、授权数） | **SKU**（轴在 SPU 声明） | **是** |
| `parameter` | 参数属性 / **基础属性** | 描述与展示、筛选（如 材质、功率、产地、保修期） | SPU（也可 SKU 级） | 否 |

除此之外，**固定语义不进属性表**，直接做系统列：`name` / `brand_id` / `model_no` /
`barcode` / `weight` / `tax_class`。这些是每个商品都有的，做成动态属性只会带来无谓的 EAV 开销。

> 判定规则（可直接落成门禁）：
> `role = 'sales'` ⇔ `is_variant_axis = true`；只有 `sales` 允许出现在 `sku_attribute` 里。

### 3.2 属性的能力字段

```sql
-- commerce_product_attribute 新增
attribute_role    TEXT NOT NULL   -- key | sales | parameter   （核心新增）
value_type        TEXT NOT NULL   -- text | number | date | boolean | enum | multi_enum | range
value_scope       TEXT NOT NULL   -- spu | sku        （该属性的值挂在哪一层）
is_variant_axis   BOOLEAN         -- 是否可作 SKU 轴（仅 sales 允许为真）
is_multi_value    BOOLEAN         -- 多值（如「适用场景」可多选）
is_free_text      BOOLEAN         -- 允许自定义值（否则必须取自值字典）
unit_code         TEXT            -- 数字型单位：g / mm / mAh / W / ml
group_id          BIGINT          -- 属性分组（输入表单分组展示）
sort_order        BIGINT          -- 表单内顺序（颜色永远第一个、规格第二个，行业惯例）
status / version / 审计字段 / deleted_at
```

约束：

```sql
ALTER TABLE commerce_product_attribute
  ADD CONSTRAINT ck_commerce_product_attribute_role
    CHECK (attribute_role IN ('key','sales','parameter')),
  ADD CONSTRAINT ck_commerce_product_attribute_value_type
    CHECK (value_type IN ('text','number','date','boolean','enum','multi_enum','range')),
  ADD CONSTRAINT ck_commerce_product_attribute_axis
    CHECK (is_variant_axis = FALSE OR attribute_role = 'sales'),
  ADD CONSTRAINT ck_commerce_product_attribute_enum_needs_dict
    CHECK (value_type NOT IN ('enum','multi_enum') OR is_free_text = FALSE);
```

### 3.3 类目属性模板（让「该填什么」由类目决定）

```sql
-- commerce_product_category_attribute（表已在 cloudrouter，建议迁回 merchandise）
category_id, attribute_id,
attribute_role      TEXT,      -- 覆盖属性自身角色：同一「颜色」在服装= sales，在家具= parameter
spu_required        BOOLEAN NOT NULL DEFAULT FALSE,   -- SPU 层必填
sku_required        BOOLEAN NOT NULL DEFAULT FALSE,   -- SKU 层必填（通常 sales 轴为真）
is_filterable       BOOLEAN NOT NULL DEFAULT FALSE,   -- 前台可筛选
is_searchable       BOOLEAN NOT NULL DEFAULT FALSE,   -- 进入搜索倒排
is_variant_axis     BOOLEAN NOT NULL DEFAULT FALSE,   -- 本类目下这条属性是不是规格轴
default_value_id    BIGINT,
inherit_from_parent BOOLEAN NOT NULL DEFAULT TRUE,    -- 子类目继承父类目模板
sort_order,
CONSTRAINT ux_... UNIQUE (tenant_id, category_id, attribute_id)
```

**模板继承规则**：取「自身 + 所有祖先类目」的并集，`category_id` 越深优先级越高（覆盖 `default`/`required`/`role`）。

### 3.4 属性值字典与标准化

```sql
-- commerce_product_attribute_value 新增
color_hex   TEXT,     -- 色卡（颜色类属性必备，前台直接渲染）
image_uri   TEXT,     -- 规格图（尺码表/款式图）
alias_json  JSONB,    -- 同义词（"纯黑/全黑" → 归一到"黑色"），根治值不统一
```

> 行业痛点「同一个颜色写了 黑色 / 纯黑 / 全黑」的解法：值字典 + `alias_json` 归一 +
> 门禁禁止 `is_free_text = false` 的属性出现非字典值。

### 3.5 属性值落库（当前最大的缺口）

**基础属性（SPU 层）——新增表**：

```sql
CREATE TABLE commerce_product_spu_attribute (
  id                 BIGINT NOT NULL PRIMARY KEY,
  uuid               UUID   NOT NULL,
  tenant_id          BIGINT NOT NULL,
  organization_id    BIGINT NOT NULL DEFAULT 0,
  spu_id             BIGINT NOT NULL,
  attribute_id       BIGINT NOT NULL,
  attribute_value_id BIGINT,          -- enum/multi_enum/range 型取值
  value_key          TEXT   NOT NULL, -- 去重键：字典值取 attribute_value_id，自定义值取归一化文本
  custom_value_text  TEXT,            -- text 型
  custom_value_number NUMERIC,        -- number / range 型
  custom_value_unit  TEXT,            -- 单位（冗余快照，属性改了不影响历史）
  custom_value_date  DATE,            -- date 型
  sort_order         BIGINT NOT NULL DEFAULT 0,
  created_at         TIMESTAMPTZ NOT NULL,
  updated_at         TIMESTAMPTZ NOT NULL,
  CONSTRAINT ux_commerce_product_spu_attribute
    UNIQUE (tenant_id, spu_id, attribute_id, value_key)   -- 多值天然支持：value_key 不同即多行
);
CREATE INDEX idx_commerce_product_spu_attribute_filter
  ON commerce_product_spu_attribute (tenant_id, attribute_id, attribute_value_id);
```

**交易属性（SKU 层）——复用已有表，补约束**：

```sql
-- commerce_product_sku_attribute（表已在 cloudrouter，建议迁回 merchandise）
CONSTRAINT ux_commerce_product_sku_attribute UNIQUE (tenant_id, sku_id, attribute_id)
-- 每个轴一个 SKU 只能取一个值；且 attribute_id 必须是 role='sales' 的属性（应用层 + 门禁校验）
```

---

## 4. SPU 设计

**职责**：商品身份的载体 + 基础属性 + 类目陈列 + 媒体 + 翻译。**不含**价格、库存、规格。

```sql
ALTER TABLE commerce_product_spu
  ADD COLUMN uuid            UUID   NOT NULL,
  ADD COLUMN brand_id        BIGINT,
  ADD COLUMN model_no        TEXT,                    -- 型号（品牌下的标准型号，行业必备）
  ADD COLUMN spu_type        TEXT   NOT NULL DEFAULT 'normal',
      -- normal | bundle(组合装) | gift(赠品) | virtual | service
  ADD COLUMN sales_status    TEXT   NOT NULL DEFAULT 'on_sale'
      CHECK (sales_status IN ('on_sale','off_sale','pre_sale','sold_out')),
  ADD COLUMN spec_json       JSONB  NOT NULL DEFAULT '{}'::jsonb,
      -- 降级为「扩展元数据 / 供应商透传」；不再是规格或 i18n 的载体
  ADD COLUMN version         BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN deleted_at      TIMESTAMPTZ,
  ADD COLUMN created_by      BIGINT, ADD COLUMN updated_by BIGINT;

ALTER TABLE commerce_product_spu
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at::timestamptz,
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING updated_at::timestamptz,
  ADD CONSTRAINT ck_commerce_product_spu_status
    CHECK (status IN ('draft','active','inactive','archived','deleted'));

CREATE UNIQUE INDEX ux_commerce_product_spu_no
  ON commerce_product_spu (tenant_id, organization_id, spu_no) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_commerce_product_spu_uuid ON commerce_product_spu (uuid);
CREATE INDEX idx_commerce_product_spu_list
  ON commerce_product_spu (tenant_id, organization_id, status, id);  -- 补唯一键兜底
```

**类目陈列**：`commerce_product_spu_category`（已有）承担多对多 + `primary_flag`。
建议约束「每个 SPU 至多一个 `primary_flag = 1`」：

```sql
CREATE UNIQUE INDEX ux_commerce_product_spu_category_primary
  ON commerce_product_spu_category (tenant_id, spu_id) WHERE primary_flag = 1;
```

**媒体**：`commerce_product_media`（已有，符合 `MEDIA_RESOURCE_SPEC.md` §6）保持不变，
仅需把 `resource_snapshot JSONB` 的使用规范化，并补 `sort_order` 的排序兜底。

---

## 5. SKU 设计

**职责**：可售/可库存/可计价的最小单元。**身份 = 交易属性取值组合**。

```sql
ALTER TABLE commerce_product_sku
  -- 标识
  ADD COLUMN uuid            UUID NOT NULL,
  ADD COLUMN barcode         TEXT,        -- GTIN-8/12/13/14（MEDIA_RESOURCE_SPEC §6 与 GS1 要求）
  -- 金额（统一 minor + 币种 + scale）
  ADD COLUMN list_price_minor  BIGINT,    -- 标价/原价
  ADD COLUMN sale_price_minor  BIGINT,    -- 售价（默认取 list）
  ADD COLUMN cost_price_minor  BIGINT,    -- 成本价（毛利/降价分析）
  ADD COLUMN price_scale       SMALLINT NOT NULL DEFAULT 2,   -- JPY/KRW 为 0
  -- 物流 / 税务
  ADD COLUMN weight_g   BIGINT,
  ADD COLUMN length_mm  BIGINT, ADD COLUMN width_mm BIGINT, ADD COLUMN height_mm BIGINT,
  ADD COLUMN tax_class_code    TEXT,
  ADD COLUMN hs_code           TEXT,      -- 跨境报关
  -- 库存声明（注意：只是声明，不是事实）
  ADD COLUMN inventory_tracking TEXT NOT NULL DEFAULT 'untracked',
      -- tracked(按库存售卖) | untracked(无限售/虚拟) —— 已有列，补 CHECK
  ADD COLUMN inventory_policy   TEXT NOT NULL DEFAULT 'deny',
      -- deny(库存不足即拒单) | continue(允许超卖/预订)
  -- 交易约束（行业必备，当前全缺）
  ADD COLUMN min_order_quantity  BIGINT NOT NULL DEFAULT 1,   -- 起订量
  ADD COLUMN max_order_quantity  BIGINT,                      -- 单笔限购
  ADD COLUMN step_quantity       BIGINT NOT NULL DEFAULT 1,   -- 步进（按箱/按打）
  ADD COLUMN package_unit        TEXT,                        -- 销售单位（箱/盒/件）
  ADD COLUMN units_per_package   BIGINT,                      -- 装箱数
  -- 序列化 / 效期（3C、食品）
  ADD COLUMN serial_required     BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN shelf_life_days     BIGINT,
  -- 生命周期
  ADD COLUMN spec_json        JSONB NOT NULL DEFAULT '{}'::jsonb,  -- 由 text 改 jsonb
  ADD COLUMN variant_signature TEXT NOT NULL,   -- 交易属性取值组合的稳定签名
  ADD COLUMN version          BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN deleted_at       TIMESTAMPTZ,
  ADD COLUMN created_by BIGINT, ADD COLUMN updated_by BIGINT;

ALTER TABLE commerce_product_sku
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at::timestamptz,
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING updated_at::timestamptz,
  ADD CONSTRAINT ck_commerce_product_sku_price_nonneg
    CHECK ((list_price_minor IS NULL OR list_price_minor >= 0)
       AND (sale_price_minor IS NULL OR sale_price_minor >= 0)
       AND (cost_price_minor IS NULL OR cost_price_minor >= 0)),
  ADD CONSTRAINT ck_commerce_product_sku_quantity_rules
    CHECK (min_order_quantity >= 1
       AND step_quantity >= 1
       AND (max_order_quantity IS NULL OR max_order_quantity >= min_order_quantity)),
  ADD CONSTRAINT ck_commerce_product_sku_inventory
    CHECK (inventory_tracking IN ('tracked','untracked')),
  ADD CONSTRAINT ck_commerce_product_sku_dims
    CHECK (weight_g IS NULL OR weight_g >= 0);

CREATE UNIQUE INDEX ux_commerce_product_sku_no
  ON commerce_product_sku (tenant_id, organization_id, sku_no) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_commerce_product_sku_barcode
  ON commerce_product_sku (barcode) WHERE barcode IS NOT NULL AND deleted_at IS NULL;
-- 核心：同一 SPU 下规格组合不得重复
CREATE UNIQUE INDEX ux_commerce_product_sku_variant
  ON commerce_product_sku (tenant_id, spu_id, variant_signature) WHERE deleted_at IS NULL;
CREATE INDEX idx_commerce_product_sku_spu_list
  ON commerce_product_sku (tenant_id, organization_id, spu_id, status, id);
```

### 5.1 `variant_signature` —— 规格去重的关键

`variant_signature` = 把该 SKU 在**所有销售轴**上的 `attribute_id:attribute_value_id` 按
`attribute_id` 升序排序后拼接（或取其 sha256 短哈希）。三个作用：

1. **唯一约束**：同一 SPU 下不可能出现两个规格完全相同的 SKU（当前完全没有这层保护）。
2. **规格组合校验**：新建 SKU 时，SPU 声明的每个销售轴都必须有取值，缺一即报错
   （否则会出现「颜色=黑、尺码=NULL」这种半成品 SKU）。
3. **前台规格选择器**：直接由签名反解出「颜色 × 尺码」矩阵，标出哪些组合已建 SKU。

> 这一条是 SPU/SKU 模型真正成立的标志。当前 `spec_json` 自由 JSON 无法提供任何一项。

### 5.2 商品侧与库存侧的接口约定

| 商品侧（merchandise） | 库存侧（inventory） |
|---|---|
| `sku.inventory_tracking` = `tracked` / `untracked` | `tracked` 的 SKU 必须存在 stock 行；`untracked` 不建 stock 行 |
| `sku.inventory_policy` = `deny` / `continue` | `deny` → 可用量不足即拒；`continue` → 允许负可用量 |
| `sku.min/max/step_order_quantity` | 库存校验需按 `step_quantity` 对齐（校验在商品侧，扣减在库存侧） |
| 商品侧**不存**任何库存数量 | 库存侧**不判断**商品状态，由调用方传入 |

---

## 6. 价格体系设计

### 6.1 两层价格，不要堆列

| 层 | 存放 | 语义 |
|---|---|---|
| 基准价 | `commerce_product_sku.list_price_minor` / `sale_price_minor` / `cost_price_minor` | 人人都有的默认价 |
| 覆盖价 | `commerce_price_list_item` | 按 市场 × 客群 × 阶梯量 × 时间窗 覆盖基准价 |

**明确反对**在建表时加 `member_price` / `activity_price` / `wholesale_price` 列。
每加一种价格就要加一列、就要改一次表，且无法表达时间窗与阶梯。行业里 Magento 用
tier price 子表、Shopify 用 price list，都是这个结论。

### 6.2 价目表明细（当前「有头无明细」的补全）

```sql
CREATE TABLE commerce_price_list_item (
  id              BIGINT NOT NULL PRIMARY KEY,
  uuid            UUID   NOT NULL,
  tenant_id       BIGINT NOT NULL,
  organization_id BIGINT NOT NULL DEFAULT 0,
  price_list_id   BIGINT NOT NULL,
  sku_id          BIGINT NOT NULL,
  price_minor     BIGINT NOT NULL CHECK (price_minor >= 0),
  currency_code   CHAR(3) NOT NULL,
  price_scale     SMALLINT NOT NULL DEFAULT 2,
  min_quantity    BIGINT NOT NULL DEFAULT 1 CHECK (min_quantity >= 1),  -- 阶梯价
  starts_at       TIMESTAMPTZ,
  ends_at         TIMESTAMPTZ,
  status          TEXT NOT NULL DEFAULT 'active'
      CHECK (status IN ('active','inactive','expired')),
  version         BIGINT NOT NULL DEFAULT 0,
  created_at      TIMESTAMPTZ NOT NULL,
  updated_at      TIMESTAMPTZ NOT NULL,
  CONSTRAINT ux_commerce_price_list_item
    UNIQUE (tenant_id, price_list_id, sku_id, min_quantity),
  CONSTRAINT ck_commerce_price_list_item_window
    CHECK (ends_at IS NULL OR starts_at IS NULL OR ends_at > starts_at),
  CONSTRAINT ck_commerce_price_list_item_currency
    CHECK (currency_code ~ '^[A-Z]{3}$')
);
-- 价格解析热路径：按 sku 找「当前有效」的明细
CREATE INDEX idx_commerce_price_list_item_resolve
  ON commerce_price_list_item (tenant_id, organization_id, sku_id, status, min_quantity);
```

`commerce_price_list` 本身补 `version` / `deleted_at` / 审计字段，并把 `customer_segment`
与 `market_code` 的取值改为引用注册表（当前是自由文本）。

### 6.3 价格解析优先级（交给 catalog / order，不在商品侧算）

```text
1. price_list_item 精确匹配（market + segment + quantity>=min_quantity + 当前时间窗）
     ↓ 无命中
2. sku.sale_price_minor
     ↓ 无
3. sku.list_price_minor
```

**并发与缓存**：价格解析结果不落商品库；由结算侧按请求缓存，键含 `market+segment+quantity+now`。

---

## 7. 库存体系设计（owner = `sdkwork-inventory`）

> **本节的表不建在 merchandise。** 商品侧只保留 §5.2 的声明字段。
> 库存事实表已存在于 `sdkwork-inventory/database/ddl/baseline/postgres/0001_inventory_baseline.sql`
> （`commerce_inventory_stock` 17 列 / `commerce_inventory_reservation` 24 列）。
> 以下是对该仓的优化建议。

### 7.1 已发现的三个缺陷

**缺陷 1：`commerce_inventory_stock` 没有唯一键 —— 同一 SKU 同一仓可以被建出多行。**

```sql
-- 现有（只有普通索引，无唯一约束）
CREATE INDEX idx_commerce_inventory_stock_sku
    ON commerce_inventory_stock (tenant_id, sku_id, status);
```

而扣减逻辑是：

```sql
-- sdkwork-order/.../inventory.rs:163
SELECT ... FROM commerce_inventory_stock
 WHERE tenant_id=$1 AND organization_id=$2 AND shop_id=$3 AND sku_id=$4 AND status='active'
   AND available_quantity - safety_stock_quantity >= $5
 ORDER BY available_quantity DESC, id LIMIT 1 FOR UPDATE
```

`LIMIT 1` + `ORDER BY available_quantity DESC` 的存在，本身就是「承认可能有多行」。
后果：库存被摊到多个桶，扣减只动其中一个 → **超卖或误判不可售**。

**缺陷 2：`commerce_inventory_reservation` 的 `idempotency_key` 没有唯一约束。**

代码到处依赖它做幂等（`UPDATE ... WHERE id=$5 AND status='reserved'` 并写入 `idempotency_key`），
但 DB 层没有去重边界 → 违反 DB010「Idempotent flows have a unique dedupe boundary」，
并发重试可以插出两条预约。`reservation_no` 同样无唯一约束。

**缺陷 3：只有计数、没有流水。** 全工作区不存在「库存变动明细」表，
`on_hand / available / locked / reserved / sold / safety_stock` 六个计数列互相派生，
一旦被写歪就无法追溯、无法对账、无法重建。

### 7.2 建议的修正

```sql
-- (1) 把可选维度归一为 sentinel，消除「多桶」并加唯一键
ALTER TABLE commerce_inventory_stock
  ALTER COLUMN shop_id SET DEFAULT '',
  ALTER COLUMN warehouse_id SET DEFAULT '',
  ALTER COLUMN fulfillment_node_id SET DEFAULT '';
UPDATE commerce_inventory_stock SET shop_id='' WHERE shop_id IS NULL;
UPDATE commerce_inventory_stock SET warehouse_id='' WHERE warehouse_id IS NULL;
UPDATE commerce_inventory_stock SET fulfillment_node_id='' WHERE fulfillment_node_id IS NULL;
ALTER TABLE commerce_inventory_stock
  ALTER COLUMN shop_id SET NOT NULL,
  ALTER COLUMN warehouse_id SET NOT NULL,
  ALTER COLUMN fulfillment_node_id SET NOT NULL,
  ALTER COLUMN sku_id SET NOT NULL;          -- 现有列可空，是明显漏洞
CREATE UNIQUE INDEX ux_commerce_inventory_stock_scope
  ON commerce_inventory_stock
     (tenant_id, organization_id, shop_id, warehouse_id, fulfillment_node_id, sku_id);
-- 唯一键建好后，hot query 去掉 ORDER BY ... LIMIT 1，改为按唯一键单行 SELECT ... FOR UPDATE

-- (2) 计数非负 + 派生列一致性
ALTER TABLE commerce_inventory_stock
  ADD CONSTRAINT ck_commerce_inventory_stock_nonneg
    CHECK (on_hand_quantity >= 0 AND available_quantity >= 0 AND locked_quantity >= 0
       AND reserved_quantity >= 0 AND sold_quantity >= 0 AND safety_stock_quantity >= 0),
  ADD CONSTRAINT ck_commerce_inventory_stock_derived
    CHECK (available_quantity <= on_hand_quantity),
  ADD CONSTRAINT ck_commerce_inventory_stock_updated
    CHECK (updated_at >= created_at);

-- (3) 预约表的幂等边界
CREATE UNIQUE INDEX ux_commerce_inventory_reservation_no
  ON commerce_inventory_reservation (tenant_id, reservation_no);
CREATE UNIQUE INDEX ux_commerce_inventory_reservation_idem
  ON commerce_inventory_reservation (tenant_id, idempotency_key);
-- 并把这个热查询的索引补全（现有索引只覆盖 tenant+order，缺 status）
CREATE INDEX idx_commerce_inventory_reservation_order_status
  ON commerce_inventory_reservation (tenant_id, order_id, status, sku_id);

-- (4) 补 append-only 流水（本次最重要的新增）
CREATE TABLE commerce_inventory_movement (
  id              BIGINT NOT NULL PRIMARY KEY,
  uuid            UUID   NOT NULL,
  tenant_id       BIGINT NOT NULL,
  organization_id BIGINT NOT NULL DEFAULT 0,
  movement_no     TEXT   NOT NULL,
  sku_id          BIGINT NOT NULL,
  shop_id         TEXT   NOT NULL DEFAULT '',
  warehouse_id    TEXT   NOT NULL DEFAULT '',
  fulfillment_node_id TEXT NOT NULL DEFAULT '',
  movement_type   TEXT   NOT NULL
      CHECK (movement_type IN ('inbound','outbound','reserve','release','consume',
                               'restock','adjust','transfer_in','transfer_out','scrap')),
  quantity_delta  BIGINT NOT NULL,          -- 带符号，可为负
  quantity_after  BIGINT NOT NULL,          -- 变动后快照，便于对账
  source_type     TEXT   NOT NULL,          -- order | manual | purchase | return
  source_id       TEXT,
  idempotency_key TEXT   NOT NULL,
  operator_type   TEXT   NOT NULL DEFAULT 'system',   -- system | user | job
  operator_id     BIGINT,
  occurred_at     TIMESTAMPTZ NOT NULL,
  created_at      TIMESTAMPTZ NOT NULL,
  CONSTRAINT ux_commerce_inventory_movement_no UNIQUE (tenant_id, movement_no),
  CONSTRAINT ux_commerce_inventory_movement_idem UNIQUE (tenant_id, idempotency_key)
);
CREATE INDEX idx_commerce_inventory_movement_sku
  ON commerce_inventory_movement (tenant_id, organization_id, sku_id, occurred_at DESC, id DESC);
```

**口径建议**：把 `on_hand_quantity` 作为唯一事实（+ `safety_stock_quantity` 作为策略），
`available / reserved / sold / locked` 改为由 ledger 推导（物化视图或定时校验任务），
并对账任务断言 `stock.available = stock.on_hand - stock.locked - stock.reserved`。
这样「库存不准」从不可见变为可发现。

### 7.3 索引优化说明

现有 `(tenant_id, sku_id, status)` 无法支撑热查询（热查询还带 `organization_id` 与 `shop_id`）。
按 §10「Composite index order MUST be derived from equality predicates …」：

```sql
CREATE INDEX idx_commerce_inventory_stock_pick
  ON commerce_inventory_stock (tenant_id, organization_id, sku_id, status, shop_id);
```
并按 DB078 补 `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` 证据。

---

## 8. 类目体系设计

（承接审计报告 §B 类问题，这里给目标态）

```sql
ALTER TABLE commerce_product_category
  ADD COLUMN tree_type       TEXT NOT NULL DEFAULT 'backend'
      CHECK (tree_type IN ('backend','frontend')),   -- 后台管理类目 / 前台展示类目
  ADD COLUMN category_type   TEXT NOT NULL DEFAULT 'normal'
      CHECK (category_type IN ('normal','industry')),-- 普通类目 / 行业标准类目
  ADD COLUMN is_leaf         BOOLEAN NOT NULL DEFAULT TRUE,
  ADD COLUMN brand_required  BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN image_media_id  BIGINT,
  ADD COLUMN version         BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN deleted_at      TIMESTAMPTZ,
  ADD COLUMN created_by BIGINT, ADD COLUMN updated_by BIGINT;

CREATE UNIQUE INDEX ux_commerce_product_category_no
  ON commerce_product_category (tenant_id, organization_id, tree_type, category_no)
  WHERE deleted_at IS NULL;
CREATE INDEX idx_commerce_product_category_tree
  ON commerce_product_category (tenant_id, tree_type, parent_id, sort_order, id);  -- 补 id 兜底
```

**四条必须修好的树不变量**（应用层 + 门禁）：

| # | 不变量 | 为什么 |
|---|---|---|
| 1 | 改 `parent_id` 必须**在同一事务内重算自身 `path`/`level_no` 并级联重写所有子孙 `path`** | 当前完全不重算 → 子树查询全错 |
| 2 | `parent_id` 必须真实存在；不存在则**报错**，不得静默降级为根 | 当前静默降级 → 静默数据错位 |
| 3 | 禁止把 `parent_id` 指向自身或自己的子孙（环检测） | 当前无检测 |
| 4 | 有子类目、或有 SPU 陈列、或有属性绑定绑定时，**禁止删除**（先校验后 `deleted_at`） | 当前直接置 `status='deleted'` → 悬挂 |

**双树的意义**：`tree_type='frontend'` 是给用户看的（可运营调整），`backend` 是给运营录入商品用的
（属性模板挂在 `backend` 树上，变更成本高）。行业（淘宝/京东）两者分离，正是因为
「改前台导航不该影响商品录入模板」。

---

## 9. 能力边界与所有权治理

这一节是「优化」里性价比最高、也最需要 owner 裁决的部分。

### 9.1 前缀注册粒度错了（8 个仓都注册 `commerce_`）

实测：`catalog / inventory / invoice / merchandise / order / payment / shop` 七个仓
在 `database/contract/prefix-registry.json` 里都写 `prefix: "commerce_"`，
cloudrouter 额外写 `commerce_product_`。

问题：**`commerce_` 是域级前缀**（`DOMAIN_SPEC.md` 明确 `shop`/`catalog`/`merchandise`
留在 `commerce` 域内），把它注册到某个能力名下，注册表就失去了「表级所有权仲裁」的能力 ——
所以才会出现 cloudrouter 抢注 `commerce_product_` 而没人能判它错。

**建议**：注册精度下沉到**表族前缀**：

| 仓 | 当前注册 | 建议注册 |
|---|---|---|
| `sdkwork-merchandise` | `commerce_` | `commerce_product_`、`commerce_price_` |
| `sdkwork-inventory` | `commerce_` | `commerce_inventory_` |
| `sdkwork-catalog` | `commerce_` | `commerce_cart`、`commerce_cart_item`、`commerce_user_address` |
| `sdkwork-order` | `commerce_` | `commerce_order_`、`commerce_checkout_`、`commerce_shipment_`、`commerce_after_sales_`、`commerce_fulfillment_` |
| `sdkwork-payment` | `commerce_` | `commerce_payment_`、`commerce_refund_` |
| `sdkwork-cloudrouter` | `commerce_product_` | **移除**（表族属 merchandise），保留 `ai_` / `iam_user_` / `integration_` |

配套加一条工作区级门禁：**同一表族前缀不得出现两个 owner**（当前实测会立刻变红）。

### 9.2 merchandise 越界写 catalog 的三张表

实测 `crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog.rs` 中对以下表有读写：

```text
FROM/INTO/UPDATE commerce_cart            1/1
FROM/INTO/UPDATE commerce_cart_item       3/1/1
FROM/UPDATE      commerce_user_address    2/5/1
```

而这三张表的 owner 是 `sdkwork-catalog`（其 `table-registry.json` 明示），
且 merchandise 的 `http_route_manifest.rs` 里 **cart/address 路由数 = 0**。

结论：**merchandise 仓储层实现了属于 catalog 能力的购物车与收货地址，但没有任何对外路由，
且 merchandise 自己的 baseline 里根本没有这三张表** —— 既是能力边界渗漏，也是跑不起来的死代码。
建议从 `CommerceCatalogStore` / `ports/mod.rs` 中移除 cart/address 的记录与端口，
购物车与地址统一由 `sdkwork-catalog` 承担。

### 9.3 关系表的归属（承接审计报告 §E）

4 张关系表（`spu_category` / `category_attribute` / `sku_attribute` / `media`）的 DDL 在
cloudrouter baseline + migration 0044，但：
- 契约归属在 merchandise（route module + operationId 都是 merchandise 的）
- 权威契约在 shop（`sdkwork-shop-backend-api`）

**推荐**：全部迁回 merchandise（本设计方案的 §3.5 / §4 / §5 都依赖它们同库）。
理由：这三者都是商品主数据的一部分，与 `commerce_product_spu/sku` 有强一致性要求
（`variant_signature` 校验、模板必填校验都要跨表读），跨仓无法在一个事务里完成。
cloudrouter 改为只读消费，并由其自身的读模型承接。

### 9.4 capability token 的使用（需 owner 裁决，不建议擅自改）

`DOMAIN_SPEC.md` 规定：`catalog` 与 `merchandise` 是**同级能力**，
「Do not use `product` as a catch-all for both」。

现状：merchandise 的服务契约名是 `commerce.catalog`、路由全在 `/backend/v3/api/catalog/*`、
operationId 前缀是 `catalog.*`。即 **merchandise 能力顶替了 catalog 的命名**。

建议（破坏性变更，须走 expand/contract + SDK 版本）：
- 新增 `/backend/v3/api/merchandise/*` 别名与 `merchandise.*` operationId；
- `/catalog/*` 标记 deprecated，保留一个发布周期的兼容窗口；
- 服务契约名由 `commerce.catalog` 改为 `commerce.merchandise`。

> 若不改，至少要在 PRD 里显式写明「本仓的 `/catalog/*` 是管理面，catalog 能力的 C 端面在
> `sdkwork-catalog` 的 `/app/v3/api/catalog/*`」，把这条边界写死。

---

## 10. 落地路线

| 波次 | 内容 | 改表 | 验收 |
|---|---|---|---|
| **W0** 零 schema，立刻可做 | ① 移除 cart/address 死代码（§9.2）② 修类目树 4 条不变量（§8）③ `ORDER BY` 补 `id` 兜底 ④ 撤下或实现 4 个未实现端点 ⑤ `commerce_` 前缀注册下沉 + 工作区门禁 | ❌ | `cargo test --workspace` + `pnpm verify` 绿；前缀门禁能抓出 cloudrouter 与 merchandise 的 `commerce_product_` 冲突 |
| **W1** 门禁先行 | 扩写 `check-database-framework-standard.mjs`：主键类型 / 时间类型 / 金额 minor+scale / 自然键唯一 / 状态 CHECK / 排序兜底 / JSON 不承载核心字段 / **route↔OpenAPI↔handler↔表 闭环** | ❌ | 每条新断言做一次变异并确认变红（见 `gate-mutation-verification`） |
| **W2** 属性体系（本方案核心） | 新增 `commerce_product_spu_attribute`、`attribute_role`/`value_type` 放开、`category_attribute` 模板继承、`attribute_value` 补色卡/别名；`sku_attribute` 加 `role='sales'` 校验 | ✅ ADD | 能建出「颜色×尺码」4 个 SKU，`variant_signature` 唯一键能拒绝重复组合 |
| **W3** 类目模板 + 规格闭环 | 类目-属性模板可继承；新建商品按类目拉必填清单；SPU/SKU 保存时校验模板完整性 | ✅ ADD | 必填缺失必须报错；`primary_flag` 唯一键生效 |
| **W4** 价格 + 翻译 | `commerce_price_list_item`；`*_translation` 全族；`price_amount` → `price_minor + price_scale` 双写回填 | ✅ ADD→DROP | 阶梯价/客群价解析用例通过；zh-CN 与 en-US 可并存查询 |
| **W5** 关系表迁回 merchandise | migration 0044 的 4 张表迁入；cloudrouter 改只读 | ✅ 跨仓 | 三仓契约校验绿；`pnpm api:assembly:validate` 绿 |
| **W6** 库存优化（inventory 仓） | §7.2 的唯一键 + CHECK + ledger + 索引；补 `EXPLAIN ANALYZE` 证据 | ✅ | 并发扣减压测无超卖；`stock` 与 `movement` 汇总对账一致 |

---

## 11. 字段级映射（旧 → 新）

| 旧 | 新 | 迁移动作 |
|---|---|---|
| `product_spu.category_id` | `commerce_product_spu_category`（多对多 + `primary_flag`） | 数据搬迁后旧列只读一周期再删 |
| `product_spu.name` | 删除（死列，无读取路径） | 直接 DROP（先确认 seed 不再写） |
| `product_spu.spec_json`（不存在） | `product_spu.spec_json JSONB` | 新增，语义限定为「扩展元数据」 |
| `product_sku.spec_json`（规格 + i18n 混装） | 拆三处：`sku_attribute`（规格）/ `*_translation`（i18n）/ `spec_json`（扩展） | 按 key 分类搬迁；`tags` 迁到翻译表 |
| `product_sku.price_amount`（TEXT 分） | `list_price_minor BIGINT` + `price_scale` | 直接 cast，双写校验后切读 |
| `product_sku.original_price_amount` | `list_price_minor`（原价）/ `sale_price_minor`（售价） | 需业务确认哪个是哪个（当前命名与语义不一致） |
| `product_sku.sales_status`（从不被写） | 保留 + 打通写入路径（`on_sale/off_sale/pre_sale/sold_out`） | 加 CHECK，补 update 命令字段 |
| `product_attribute.value_type/scope`（硬编码） | `attribute_role` / `value_type` / `value_scope`（可设） | 补 API 字段，回填现有值 |
| `product_category.path/level_no`（不重算） | 保留但由应用强制重算 + 门禁 | 一次性全量重算修正历史偏差 |
| `price_list.customer_segment`（自由文本） | 引用注册表 | 收敛取值集合 |

---

## 12. 验收清单（可直接转成测试）

1. 同 SPU 下两个 SKU 规格组合完全相同 → **必须报唯一约束冲突**。
2. SKU 缺少 SPU 声明的某个销售轴取值 → **必须报错**。
3. `attribute_role='parameter'` 的属性出现在 `sku_attribute` → **必须报错**。
4. `value_type='enum'` 且 `is_free_text=false` 的属性写入非字典值 → **必须报错**。
5. 移动类目到新父节点后，自身与所有子孙的 `path`/`level_no` 与新位置一致。
6. 把类目父指向自己的子孙 → **必须报环错误**。
7. 删除有子类目的类目 → **必须报错**。
8. 双币种（CNY scale=2 / JPY scale=0）解析价格，结果不经 `/100` 且金额正确。
9. `price_list_item` 阶梯价：量 1 与量 100 命中不同明细；时间窗过期条目不被命中。
10. 库存：同 SKU 同仓并发 100 次扣减，`on_hand` 不为负且 `stock` 与 `movement` 汇总一致。
11. i18n：zh-CN 与 en-US 同时生效，查 en-US 不返回中文。
12. 工作区门禁：同一表族前缀出现两个 owner → **必须变红**。

---

## 13. 未决问题（需 owner 裁决）

1. **关系表归属**：4 张关系表迁回 merchandise（推荐）还是留 cloudrouter 并在契约标注 `physical_owner`？
2. **capability token**：`/catalog/*` → `/merchandise/*` 是否启动改名？涉及已发布 SDK 的破坏性变更。
3. **`price_amount` 语义**：现有 `'640'` 到底是 6.40 元还是 640 元？`original_price_amount` 是原价还是划线价？回填前必须与业务确认。
4. **前台类目树**：本仓只服务 `/backend/v3`（管理面），`tree_type='frontend'` 是否本期做，还是留给 `sdkwork-catalog`？
5. **库存计数列**：是否接受把 `available/reserved/sold/locked` 改为 ledger 推导的物化视图？这会影响 `sdkwork-order` 的现有 SQL。
6. **条码唯一范围**：`barcode` 是否全局唯一（跨租户）？GS1 上 GTIN 是全局唯一，但多租户 SaaS 通常按租户隔离 —— 需明确。
