# 商品与类目体系审计（2026-09-23）

Status: active
Owner: SDKWork maintainers
Application: `sdkwork-merchandise`
Scope: 商品数据库设计 / 类目体系 / 属性与规格体系 / 价格体系 / 跨仓所有权 / 契约与门禁
Baseline commit: `6131b62e`（工作树干净）
Cross-repo evidence: `sdkwork-cloudrouter` @ `commerce_product_*` 关系表、`sdkwork-shop`、`sdkwork-order`、`sdkwork-specs`
Companion: `sdkwork-specs/DATABASE_SPEC.md`、`MEDIA_RESOURCE_SPEC.md` §6、`I18N_SPEC.md`

> **引用符号变更说明（2026-09-23 重构后补记）**
>
> 本审计的行号与符号引用冻结在 `6131b62e`，正文不改写。以下符号在重构中已删除，正文中引用它们
> 的位置读作"当时的实现"，不再是可跳转的定位：
>
> - `ProductCategoryDraft` / `ProductAttributeDraft` / `ProductSpuDraft` / `ProductSkuDraft`
>   —— 整套 draft 家族已删除（`sdkwork-merchandise-service/src/domain/mod.rs`）。写模型统一为
>   `Create*Command` / `Update*Command`，由 `CatalogRepositoryPort` 直接消费。
> - `commerce_product_attribute.scope` —— 已从 DDL、domain、ports 三层删除；属性语义由
>   `commerce_product_category_attribute.attribute_role` 承担。
> - `commerce_product_spu.visible_surfaces` —— 已删除；展示面属呈现层决策，不是商品主数据。
>
> 仍未闭环的条目见 `docs/architecture/tech/TECH_ARCHITECTURE.md` §9。

---

## 0. 结论先行

**当前商品体系是「单 SKU 服务型商品」的骨架，不是可承载实物/多规格/多市场价格体系的商品中心。
以行业最专业商品体系（SPU-销售属性-SKU 三层 + 类目模板 + 价目表明细 + 类型化属性字典）为尺，
本设计只完成了约 40%，且存在 3 个 P0 级结构缺陷。**

一句话根因：**这套 schema 是按「一个 SPU 挂一个 SKU 的服务包」这一条业务流反推出来的，
缺少商品中心真正的复杂度来源——销售属性组合、类目属性模板、多价格、多语言、多类目陈列。**

| 维度 | 评级 | 依据 |
|---|---|---|
| 表结构完整性（vs 行业标准） | **D** | 缺 brand / price_list_item / option / 属性值绑定 / 翻译表 / barcode / 重量体积；6 张表全是「主数据壳」 |
| 列类型与约束合规 | **D** | 6 表 3 索引 1 唯一约束 0 CHECK 0 FK；主键 `TEXT` 而非 `BIGINT`，全表 `TEXT` 时间戳；直接违反 DATABASE_SPEC §6.1/§8.2/§9/§11 |
| 类目体系设计 | **C−** | 只有邻接表；改 `parent_id` 不重算 `path`/`level_no`（子树不变量破损）；无环检测/深度上限/删除保护；无前台-后台双树 |
| 属性与规格体系 | **D+** | 缺「参数属性 vs 销售属性」二分；`value_type`/`scope` 被 SQL 硬编码为 `'enum'`/`'product'`，无 API 可设；规格只落在自由 JSON |
| 价格体系 | **D** | 有 `commerce_price_list` 头、**无明细表**；`retrieve_sku_prices` 直接返回「未实现」 |
| 跨仓所有权 | **F** | 同一 `commerce_product_` 前缀被 merchandise 与 cloudrouter **两个注册表各自主张**；10 张 product 表拆在两仓 baseline |
| i18n | **F** | locale seed 用 `UPDATE` 覆盖同一 base 列，zh-CN 与 en-US 互相覆盖，7 个语种是纸面能力 |
| 契约与实现一致性 | **F** | 4 个 `category_attributes` 端点已发布进 OpenAPI+route manifest+SDK，实现返回 storage error |
| 门禁有效性 | **F** | 上述所有违规存在的情况下 `check-database-framework-standard.mjs` 输出 `passed`（实跑 EXIT=0）——结构性假绿 |
| **可承载真实交易** | **❌ 不具备** | 见 §4 问题清单 |

---

## 1. 现状全貌

### 1.1 物理表：10 张 product 表，跨两个仓

`sdkwork-merchandise` 自己的基线（`database/ddl/baseline/postgres/0001_merchandise_baseline.sql`，124 行）：

| # | 表名 | 列数 | 说明 |
|---|---|---|---|
| 1 | `commerce_product_spu` | 15 | 商品主数据 |
| 2 | `commerce_product_sku` | 18 | 销售单元（含 `spec_json`、`sales_status`） |
| 3 | `commerce_product_category` | 12 | 类目（邻接表 + `path` + `level_no`） |
| 4 | `commerce_product_attribute` | 11 | 属性字典 |
| 5 | `commerce_product_attribute_value` | 10 | 属性值字典 |
| 6 | `commerce_price_list` | 12 | 价目表**头**（无明细） |

`sdkwork-cloudrouter` 另外持有 4 张 product 关系表（baseline + `migrations/postgres/0044_product_catalog_relation_tables.up.sql`）：

| # | 表名 | 说明 |
|---|---|---|
| 7 | `commerce_product_spu_category` | SPU ↔ 类目（有序、`primary_flag`） |
| 8 | `commerce_product_category_attribute` | 类目 ↔ 属性绑定（`required`/`searchable`/`filterable`） |
| 9 | `commerce_product_sku_attribute` | SKU 属性选择（`attribute_value_id` / `custom_value`） |
| 10 | `commerce_product_media` | SPU/SKU 媒体（符合 MEDIA_RESOURCE_SPEC §6） |

> 第 7–10 张表的归属写在 cloudrouter 自己的注释里：
> 「`sdkwork-merchandise` owns only the six *master-data* tables … It publishes no DDL for the relation tables above」
> ——即 merchandise 在契约上「拥有商品」，在物理上「只拥有 60%」。
> 这 4 张表的列设计（`primary_flag`、`required/searchable/filterable`、`custom_value`、`media_role` CHECK）
> **明显比 merchandise 自己的 6 张表更专业**，这本身就是一份旁证。

### 1.2 约束与索引实况（硬证据）

```
$ grep -c "CREATE INDEX IF NOT EXISTS" 0001_merchandise_baseline.sql   → 3
$ grep -n "CONSTRAINT|UNIQUE|CHECK|REFERENCES|FOREIGN KEY" …            → 仅 1 处
   104: CONSTRAINT ux_commerce_product_attribute_value
   105:     UNIQUE (tenant_id, attribute_id, value_code)
```

6 张表、0 个 CHECK、0 个外键、1 个唯一约束。业务号 `spu_no / sku_no / category_no /
attribute_no / price_list_no` **全部没有唯一约束**，重复业务号完全依赖应用层自觉。

### 1.3 API 面：27 条路由

`crates/sdkwork-routes-merchandise-backend-api/src/http_route_manifest.rs`：
categories 4、products 5、spus 5、skus 4、attributes 2、**category_attributes 4**、price_lists 3。

---

## 2. 对标：行业标准商品体系的分层

行业最专业商品体系（Magento/Adobe Commerce 的 EAV + attribute set + configurable product、
Shopify 的 product/option/variant + 类型化 metafield definition + Metaobject、
国内电商的「后台类目 + 前台展示类目 + 关键属性/销售属性」双线、
GS1/ETIM 的 GTIN + 标准品分类）在**分层职责**上是收敛的：

| 层 | 行业职责 | 本仓现状 |
|---|---|---|
| L1 分类字典 | 后台管理类目树 / 前台展示类目树、品牌、属性分组、属性模板（attribute set） | 只有一棵邻接表；无品牌、无分组、无模板 |
| L2 属性字典 | 属性**类型化**定义（text/number/date/enum/range + 单位）、**参数属性 vs 销售属性**二分、标准值字典 | 一张扁平 attribute，`value_type`/`scope` 硬编码 |
| L3 类目-属性绑定 | 类目决定必填/可筛选/可搜索/是否规格轴 | 表在 cloudrouter，merchandise 侧 4 个端点未实现 |
| L4 商品主数据 | SPU + 多类目陈列 + 参数属性值 + 媒体 + 翻译 | 单 `category_id`、无参数属性落库面、SPU 无 `spec_json`、无翻译 |
| L5 销售单元 | SKU + 规格组合（销售属性笛卡尔）+ barcode + 重量体积 + 成本 + 税类 | 规格塞 `spec_json`、无 barcode/重量/成本/税类 |
| L6 价格 | 价目表**头+明细**、会员价/阶梯价/区域价/活动价、多币种含 scale | 只有头，无明细；SKU 单值价格 |
| L7 搜索镜像 | 面向筛选/排序的索引镜像表 | 无（`spec_json` 无法建索引） |

---

## 3. 问题清单

### A 类 · 数据建模与类型（P0）

| ID | 问题 | 硬证据 | 违反 |
|---|---|---|---|
| **A1** | 主键用 `TEXT` 而非 `BIGINT`；且**目录 CRUD 路径自己造 id**：`postgres_catalog.rs` 内联实现 `uuid_v7()`（:1272）与 `now_iso8601()`（:1289），与同仓单 SKU 路径使用的受控 `next_entity_id` **并存两套 id/时间来源** | `id TEXT NOT NULL PRIMARY KEY`；`fn uuid_v7()` | DATABASE_SPEC §6.1「MUST use `BIGINT NOT NULL PRIMARY KEY`」「value of `id` MUST be generated by an approved SDKWork ID provider before insert … repositories MUST NOT allocate ad hoc ids」；§9 DDL 模板 |
| **A2** | 全部时间列为 `TEXT`，由应用拼 ISO 字符串 | `created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP`；`now_iso8601()` | §8.2「New L2+ PostgreSQL tables **MUST** use `TIMESTAMPTZ` for instants instead of TEXT」；「Encoding these values as generic text for cross-engine convenience is **non-compliant for new L2+ tables**」（本模块 `compliance_level: L2`） |
| **A3** | 租户/组织主体列用 `TEXT` | `tenant_id TEXT`、`organization_id TEXT DEFAULT '0'` | §6.4「`tenant_id`, `organization_id` … **MUST** be SQL `BIGINT`/logical `int64`」「MUST store resolved numeric subject ids … **not client-provided opaque strings**」 |
| **A4** | 金额列 `TEXT`，无 `NUMERIC`、无精度/标度声明、无币种指数关联（JPY/KRW 为 0 位小数，用同一「分」约定会错 100 倍） | `price_amount TEXT`；order 侧 `format_money_minor` 硬编码 `abs/100` 与 `{:02}` | DB052（money MUST declare currency, precision, rounding mode）、DB094/DB095/DB096/DB098 |
| **A5** | `spec_json TEXT` 而非 `JSONB`，且**是规格/筛选的唯一载体** | `spec_json TEXT NOT NULL DEFAULT '{}'` | §8.2 原生 `JSONB` 要求；§13「JSON **MUST NOT** be the only storage for … high-frequency filter/sort fields」；DB040（EAV/JSON 不得承载核心字段） |
| **A6** | 无审计/生命周期字段：无 `created_by`/`updated_by`、无 `version`、无 `deleted_at`/`deleted_by`；软删除用 `status='deleted'` 且未定义删除行的唯一性行为 | `delete_spu` / `delete_category` / `delete_sku` → `SET status='deleted'` | §6.5、§6.6、DB017、DB025（并发写需 version/条件更新/唯一约束） |
| **A7** | 业务号无唯一约束 | 见 §1.2 | §11「Business uniqueness **MUST** be enforced by a database unique constraint …」DB010 |
| **A8** | 状态列是自由 `TEXT`，无 CHECK、无枚举字典 | `status TEXT NOT NULL DEFAULT 'draft'` | §11（CHECK SHOULD）、§12、DB009 |
| **A9** | 持久化了领域枚举里**不存在**的状态：`ProductStatus` 只有 `draft/active/inactive/archived`，但删除写入 `'deleted'` | `domain/mod.rs:13-18` vs `postgres_catalog.rs:175` | §12「API and SDK enum representations **MUST** stay compatible with persisted values」 |
| **A10** | 分页排序**无唯一键兜底** → 翻页可能重复/丢行 | `ORDER BY sort_order ASC, created_at ASC`、`ORDER BY created_at DESC` | §10「Stable list ordering **SHOULD** include a unique tie-breaker such as `id`」、DB026 |
| **A11** | 死列与双状态轴：`commerce_product_spu.name` 被 seed 写入但**无任何读取路径**（`map_spu_row` 只读 `title`）；`sales_status` **无任何 merchandise 写入路径**（create/update SQL 都不含该列），而 order 侧以它为售卖闸门 | `sales_status` 仅出现在 test 与 `sdkwork-order/.../postgres_recharge.rs:289 AND s.sales_status = 'active'` | §6 + 数据一致性；业务上 **SKU 一旦创建就无法通过接口下架** |
| **A12** | `visible_surfaces` 用分隔字符串存集合 | `visible_surfaces TEXT` | DB011、§13 |

### B 类 · 类目体系（P0/P1）

| ID | 问题 | 硬证据 |
|---|---|---|
| **B1** | **改父类目不重算 `path`/`level_no`，也不更新子孙 `path`** → 类目树闭包不变量直接破损 | `update_category` 的 `SET` 只有 `parent_id, name, sort_order, status, updated_at`（`postgres_catalog.rs:144-150`），`path`/`level_no` 不在其中 |
| **B2** | **父类目不存在时静默降级为根类目**（不报错） | `build_category_path` 查不到父节点时 `None => Ok("/".to_string())`（:1371） |
| **B3** | 无环检测、无深度上限、无「有子类目/有关联商品时禁止删除」；`delete_category` 仅置 `status='deleted'`，子类目悬挂 | `delete_category`（:168-187）；`ProductCategoryDraft::new` 只做非空校验（`domain/mod.rs:207-223`） |
| **B4** | 只有一棵类目树，缺行业通用的**前台展示类目 / 后台管理类目**二分，也没有类目类型、叶子标记、是否强制品牌 | `commerce_product_category` 12 列中无 `tree_type`/`is_leaf`/`category_type` |
| **B5** | `category_no` 无唯一约束、无格式规范 | 见 §1.2 |
| **B6** | `path`/`level_no` 是冗余派生列，却无一致性约束、无来源声明；`path` 用 id 拼接（`/{parent_id}/…`）非 code，跨环境不可移植 | `build_category_path` `format!("{parent_path}{pid}/")`；DB039（冗余字段须声明 source 与 schema version） |
| **B7** | 商品只能挂**一个**类目（`spu.category_id` 单值），多类目陈列不支持——而 cloudrouter 的 `commerce_product_spu_category` 恰好就是为此设计的，两边不通 | `commerce_product_spu.category_id TEXT` |

### C 类 · 属性与规格体系（P0/P1）

| ID | 问题 | 硬证据 |
|---|---|---|
| **C1** | **缺「参数属性 vs 销售属性」二分**——这是 SPU/SKU 模型成立的前提（销售属性决定 SKU 组合，参数属性仅展示） | `commerce_product_attribute` 只有 `scope`，而 `create_attribute` 把 `scope` **硬编码为 `'product'`**（`postgres_catalog.rs:249`） |
| **C2** | `value_type` 同样被硬编码为 `'enum'`，**没有任何 API 可设置** → 数字/日期/布尔/区间/多选类型全部不可用，与「类型化属性定义」（Shopify metafield definition / Magento EAV 属性类型）直接冲突 | 同上 `VALUES (… 'enum', 'product', 'active', 0, …)`；`ProductAttributeDraft`（`domain/mod.rs:72-78`）无 `value_type`/`scope` 字段 |
| **C3** | 无**选项/规格组**模型（option / option_value），SPU 级销售属性与 SKU 笛卡尔组合无表达载体；规格只能塞 SKU 的 `spec_json` | 全工作区无 `product_option` / `spec_option` 表 |
| **C4** | `spec_json` 一列混装三种东西：① 销售规格 ② i18n 展示文案（`tags`）③ 展示参数 → 语义污染 + 无法索引 + 无法校验 | seed：`'{"tags":["首年9.7折","18算力元/天"]}'` |
| **C5** | SPU **完全没有**元数据落库面（`SpuRecord` 无 spec/属性字段），SPU 级参数属性无处存放 | `ports/mod.rs:83-98` |
| **C6** | merchandise 侧的 SKU create/update **无法写入规格**：`ProductSkuDraft` 无属性字段；承载销售属性的 `commerce_product_sku_attribute` 表在 cloudrouter | `domain/mod.rs:47-59`；`create_sku` INSERT 列表 |
| **C7** | 属性缺 PIM 必备元数据：无单位（unit）、无属性分组、无属性模板（attribute set）；`required/filterable/searchable` 只存在于绑定表上，属性本身无默认语义 → 约束散落三处（attribute / category_attribute / sku_attribute），无单一事实源 | 三表列对比：`attribute` 无 `required`；`category_attribute` 有 `required/searchable/filterable`；`sku_attribute` 有 `custom_value` |
| **C8** | 属性值缺 `color_hex`/`image_uri` 等展示元数据（色卡、规格图是实物商品的刚需） | `commerce_product_attribute_value` 仅 `value_code`/`display_value`/`sort_order` |

### D 类 · 价格体系（P0/P1）

| ID | 问题 | 硬证据 |
|---|---|---|
| **D1** | 有价目表**头**，**没有明细表** → `commerce_price_list` 无法定价，是典型「有壳无肉」 | 全工作区无 `commerce_price_list_item`；`ports/mod.rs:137-143` 定义了 `PriceListItemRecord` 但 `retrieve_sku_prices` 实现为 `Err("sku price retrieval is not yet implemented for postgres")`（`catalog_store.rs:831-840`） |
| **D2** | 价格只在 SKU 上单值 → 无法表达会员价/阶梯价（`min_qty`）/区域价/活动价；`price_list` 已有 `customer_segment`/`market_code`/`starts_at`/`ends_at` 四个维度却无明细可挂 | `commerce_product_sku.price_amount` 单列 |
| **D3** | 无精度/舍入模式声明（DB052 MUST） | 见 A4 |
| **D4** | 无成本价（毛利/降价分析缺失）、无税类（tax class） | `commerce_product_sku` 18 列中无 |

### E 类 · 跨仓所有权与契约（P0，最严重）

| ID | 问题 | 硬证据 |
|---|---|---|
| **E1** | **前缀注册表直接冲突**：同一 `commerce_product_` 命名空间被两个仓以不同 owner 注册 | `sdkwork-merchandise/…/prefix-registry.json`：`commerce_` → owner `sdkwork-commerce-platform`；`sdkwork-cloudrouter/…/prefix-registry.json`：`commerce_product_` → owner `cloud-router-platform`。违反 DB064 / DB067 / DB021 |
| **E2** | merchandise 声明了**整段 `commerce_`** 前缀，但该前缀实际被四方共用（shop 的 `commerce_shop*`、order/payment 的 `commerce_payment_*`/`commerce_refund_*`、cloudrouter 的 `commerce_product_*`）→ 过度声明 | 同上 |
| **E3** | 10 张 product 表物理归属分散在两仓 baseline，而**权威契约在第三个仓**（`sdkwork-shop-backend-api`）→ 三仓强耦合、任一处变更需三仓协同，且当前无机器校验闭环 | cloudrouter migration 0044 注释自述「sdkwork-merchandise … publishes no DDL for the relation tables above」 |
| **E4** | merchandise 的 route manifest + OpenAPI 发布 `category_attributes`，但该表 DDL 在 cloudrouter → **契约归属与物理归属分离** | DB067（一个物理表映射多个实体契约须有共享语义证明）；DB059（ORM/DDL/registry/API/SDK 须同步） |

### F 类 · i18n（P0）

| ID | 问题 | 硬证据 |
|---|---|---|
| **F1** | locale seed 用 `UPDATE` **覆盖同一行同一 base 列**（`name`/`title`/`spec_json`）→ zh-CN 与 en-US 无法并存，结果取决于执行顺序（后跑者全胜） | `seeds/locales/zh-CN/001_sku_locale.sql` 与 `en-US/001_sku_locale.sql` 逐字同构，都是 `UPDATE commerce_product_sku SET name = '…'` |
| **F2** | 两个语种同时被登记进 standard profile | `seed.manifest.json`：`"standard" → locales: { zh-CN: […], en-US: […] }` |
| **F3** | 无 `<module>_<resource>_translation` 表 | §6.4 标准模板明确要求 base 表存机器字段、translation 表存本地化文本 |
| **F4** | 类目名、属性名、属性值展示名（`category.name` / `attribute.name` / `attribute_value.display_value`）**完全没有本地化面**，而 manifest 却声明 7 个 `supportedLocales`、`i18nVersion: 1.0.0` | 三张表均为单语言列 |

### G 类 · 契约宣称与实现不符 + 门禁假绿（P0）

| ID | 问题 | 硬证据 |
|---|---|---|
| **G1** | 4 个 `category_attributes` 端点已发布进 OpenAPI、route manifest、下游 SDK，**实现全部返回 storage error** | `catalog_store.rs:695-737`：`"category attribute listing/creation/update/deletion is not yet implemented for postgres"` |
| **G2** | `retrieve_sku_prices` 同上 | `catalog_store.rs:831-840` |
| **G3** | 门禁在存在 A1–A12/B5/D3 全部违规的情况下输出通过 → **结构性假绿** | 实跑：`node sdkwork-specs/tools/check-database-framework-standard.mjs --root .` → `Database framework standard passed`，EXIT=0。该脚本只校验 `organization_id` sentinel、前缀注册、drift 策略，**不校验列类型/唯一性/CHECK/索引次序/DTO 对齐** |
| **G4** | `/catalog/products` 与 `/catalog/spus` 是**两个 URL 一个实现**（都调 `list_spus_page`）→ 同一资源两套命名，客户端与运维理解成本翻倍 | `backend_catalog_router.rs:222`（products）与 `:364`（spus） |

### H 类 · 缺失的行业基础能力

| ID | 缺失 | 依据 |
|---|---|---|
| **H1** | 品牌 / 制造商表 | 行业基础表；同日竞品「品牌一致则合并 SPU」的逻辑依赖它 |
| **H2** | `barcode` / GTIN 列 | `MEDIA_RESOURCE_SPEC.md` §6 明确「SKU code, **barcode**, SKU image, price, stock, and specs are validated together」；GS1 GTIN 是条码/扫码/仓配前提 |
| **H3** | 重量 / 体积 / 包装尺寸 | 物流计费与运单前提 |
| **H4** | HS code / 原产地 / 报关要素 | 跨境必需 |
| **H5** | 商品搜索镜像表（`search_index` 角色） | §5 表角色清单 |
| **H6** | 商品与类目的多对多主次陈列（表已有，两侧不通） | 见 B7 |
| **H7** | 无 `product_type` 枚举字典（`physical/virtual/membership/points_recharge/service` 硬编码在 Rust 枚举，DB 侧无注册/无 CHECK） | DB009 |

---

## 4. 目标模型（推荐）

> 原则：**保留现有 6 张表的对外语义与路径前缀不变**（避免破坏在跑的 order/membership 消费方），
> 以「扩建 + 迁移」取代「重写」。新增列一律用规范类型，旧 `TEXT` 列降级为过渡 shadow 列，按 expand/contract 收口。

### L1 分类与属性字典

```sql
-- 类目：补类型/叶子/品牌约束，修正派生列语义
ALTER TABLE commerce_product_category
  ADD COLUMN tree_type     TEXT NOT NULL DEFAULT 'backend',   -- backend | frontend
  ADD COLUMN category_type TEXT NOT NULL DEFAULT 'normal',    -- normal | industry
  ADD COLUMN is_leaf       BOOLEAN NOT NULL DEFAULT TRUE,
  ADD COLUMN brand_required BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN version       BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN deleted_at    TIMESTAMPTZ,
  ADD COLUMN created_by    BIGINT,
  ADD COLUMN updated_by    BIGINT;
ALTER TABLE commerce_product_category
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at::timestamptz,
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING updated_at::timestamptz;
CREATE UNIQUE INDEX ux_commerce_product_category_no
  ON commerce_product_category (tenant_id, category_no) WHERE deleted_at IS NULL;
CREATE INDEX idx_commerce_product_category_parent
  ON commerce_product_category (tenant_id, parent_id, sort_order, id);

-- 类目翻译（新增；替代 locale seed 覆盖 base 列）
CREATE TABLE commerce_product_category_translation (
  id BIGINT NOT NULL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  organization_id BIGINT NOT NULL DEFAULT 0,
  category_id BIGINT NOT NULL,
  locale TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  meta_title TEXT,
  meta_description TEXT,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT ux_commerce_product_category_translation UNIQUE (tenant_id, category_id, locale)
);

-- 品牌（新增）
CREATE TABLE commerce_product_brand (
  id BIGINT NOT NULL PRIMARY KEY,
  uuid UUID NOT NULL,
  tenant_id BIGINT NOT NULL,
  organization_id BIGINT NOT NULL DEFAULT 0,
  brand_no TEXT NOT NULL,
  name TEXT NOT NULL,
  logo_media_id BIGINT,
  sort_order BIGINT NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','inactive','deleted')),
  version BIGINT NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  deleted_at TIMESTAMPTZ,
  CONSTRAINT ux_commerce_product_brand_no UNIQUE (tenant_id, brand_no),
  CONSTRAINT ux_commerce_product_brand_uuid UNIQUE (uuid)
);

-- 属性：类型化 + 参数/销售二分 + 单位 + 分组 + 规格轴
ALTER TABLE commerce_product_attribute
  ADD COLUMN attribute_kind TEXT NOT NULL DEFAULT 'parameter', -- parameter | sales
  ADD COLUMN is_variant_axis BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN is_required     BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN is_filterable   BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN is_searchable   BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN is_multi_value  BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN unit_code       TEXT,
  ADD COLUMN group_id        BIGINT,
  ADD COLUMN version         BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN deleted_at      TIMESTAMPTZ;
ALTER TABLE commerce_product_attribute
  ADD CONSTRAINT ck_commerce_product_attribute_value_type
    CHECK (value_type IN ('text','number','date','boolean','enum','multi_enum','range'));
ALTER TABLE commerce_product_attribute
  ADD CONSTRAINT ck_commerce_product_attribute_kind
    CHECK (attribute_kind IN ('parameter','sales'));
CREATE UNIQUE INDEX ux_commerce_product_attribute_no
  ON commerce_product_attribute (tenant_id, attribute_no) WHERE deleted_at IS NULL;
```

### L3 类目-属性绑定（表已存在于 cloudrouter，本节给出**归属收敛方案**）

```sql
-- 方案 A（推荐）：把 4 张 relation 表迁回 merchandise，cloudrouter 只做只读消费
--   —— 使「商品契约归属 = 商品物理归属」，一次性消除 E1/E3/E4
-- 方案 B（过渡）：表留在 cloudrouter，但
--   ① merchandise 的 prefix-registry 改为只声明自己实际拥有的表前缀（精确列表，非整段 commerce_）
--   ② 在 shop-backend-api 契约里显式标注这 4 张表的 physical_owner = cloudrouter
--   ③ 新增跨仓门禁：route manifest 的每个 operationId 必须能解析到「存在的表 + 存在的 handler」
ALTER TABLE commerce_product_category_attribute
  ADD COLUMN is_variant_axis BOOLEAN NOT NULL DEFAULT FALSE,   -- 缺失：类目层声明哪条属性是规格轴
  ADD COLUMN default_value_id BIGINT,
  ADD COLUMN inherited_from_parent BOOLEAN NOT NULL DEFAULT TRUE;
```

### L4/L5 商品主数据与销售单元

```sql
ALTER TABLE commerce_product_spu
  ADD COLUMN uuid UUID NOT NULL,
  ADD COLUMN brand_id BIGINT,
  ADD COLUMN sales_status TEXT NOT NULL DEFAULT 'on_sale'
      CHECK (sales_status IN ('on_sale','off_sale','pre_sale')),
  ADD COLUMN spec_json JSONB NOT NULL DEFAULT '{}'::jsonb,   -- SPU 级参数属性 + 展示元数据
  ADD COLUMN version BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN deleted_at TIMESTAMPTZ,
  ADD COLUMN created_by BIGINT, ADD COLUMN updated_by BIGINT;
ALTER TABLE commerce_product_spu
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at::timestamptz,
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING updated_at::timestamptz;
ALTER TABLE commerce_product_spu
  ADD CONSTRAINT ck_commerce_product_spu_status
    CHECK (status IN ('draft','active','inactive','archived','deleted'));
CREATE UNIQUE INDEX ux_commerce_product_spu_no
  ON commerce_product_spu (tenant_id, spu_no) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_commerce_product_spu_uuid ON commerce_product_spu (uuid);

ALTER TABLE commerce_product_sku
  ADD COLUMN uuid UUID NOT NULL,
  ADD COLUMN barcode TEXT,             -- GTIN-8/12/13/14
  ADD COLUMN cost_price_minor BIGINT,
  ADD COLUMN weight_g BIGINT,
  ADD COLUMN length_mm BIGINT, ADD COLUMN width_mm BIGINT, ADD COLUMN height_mm BIGINT,
  ADD COLUMN tax_class_code TEXT,
  ADD COLUMN spec_json_new JSONB NOT NULL DEFAULT '{}'::jsonb,
  ADD COLUMN version BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN deleted_at TIMESTAMPTZ,
  ADD COLUMN created_by BIGINT, ADD COLUMN updated_by BIGINT;
ALTER TABLE commerce_product_sku
  ADD CONSTRAINT ck_commerce_product_sku_price_nonneg
    CHECK (price_minor >= 0);
CREATE UNIQUE INDEX ux_commerce_product_sku_no
  ON commerce_product_sku (tenant_id, sku_no) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX ux_commerce_product_sku_barcode
  ON commerce_product_sku (barcode) WHERE barcode IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX idx_commerce_product_sku_spu_list
  ON commerce_product_sku (tenant_id, spu_id, status, id);   -- 补唯一键兜底
```

> 金额迁移：`price_amount TEXT` → `price_minor BIGINT` + `currency_code CHAR(3)` + `price_scale SMALLINT`，
> 并把「分」的硬编码语义（`/100`）改成由 `price_scale` 驱动，JPY/KRW 才能正确（DB094/DB095/DB098）。

### L6 价格体系

```sql
CREATE TABLE commerce_price_list_item (
  id BIGINT NOT NULL PRIMARY KEY,
  uuid UUID NOT NULL,
  tenant_id BIGINT NOT NULL,
  organization_id BIGINT NOT NULL DEFAULT 0,
  price_list_id BIGINT NOT NULL,
  sku_id BIGINT NOT NULL,
  price_minor BIGINT NOT NULL CHECK (price_minor >= 0),
  currency_code CHAR(3) NOT NULL,
  price_scale SMALLINT NOT NULL DEFAULT 2,
  min_quantity BIGINT NOT NULL DEFAULT 1,
  starts_at TIMESTAMPTZ,
  ends_at TIMESTAMPTZ,
  status TEXT NOT NULL DEFAULT 'active',
  version BIGINT NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT ux_commerce_price_list_item
    UNIQUE (tenant_id, price_list_id, sku_id, min_quantity),
  CONSTRAINT ck_commerce_price_list_item_window
    CHECK (ends_at IS NULL OR starts_at IS NULL OR ends_at > starts_at)
);
CREATE INDEX idx_commerce_price_list_item_sku
  ON commerce_price_list_item (tenant_id, sku_id, status, min_quantity);
```

### L7 翻译与搜索镜像

- 为 `spu` / `sku` / `attribute` / `attribute_value` / `brand` 各建 `*_translation` 表，
  唯一键统一 `(tenant_id, <resource>_id, locale)`，遵循 `I18N_SPEC.md` 的 BCP 47 与 DB §6.4 模板。
- 新增 `commerce_product_search_index`（`search_index` 角色）：承接 `attribute_kind='sales'` 的
  筛选/排序倒排，把 `spec_json` 从「唯一筛选载体」降级为「扩展元数据」。

---

## 5. 落地路径（遵守「无数据库 owner 评审不得改 schema」）

数据库改动需要 owner 评审，因此分四波推进，**前两波不改表**：

| 波次 | 内容 | 改表 | 风险 |
|---|---|---|---|
| **W0 立即可做（P0，零 schema 变更）** | ① 撤下或实现 `category_attributes` 4 个端点（G1）与 `retrieve_sku_prices`（G2）——**不得发布未实现能力**；② 调解 `commerce_product_` 前缀注册冲突（E1/E2）；③ 修 `update_category` 重算 `path`/`level_no` + 更新子孙（B1）；④ 修 `build_category_path` 父缺失改报错（B2）；⑤ 分页 `ORDER BY` 补 `id` 兜底（A10）；⑥ `/catalog/products` 与 `/spus` 二选一或明确别名关系（G4）；⑦ 给全部 locale seed 加互斥/顺序声明（F1/F2 缓解） | ❌ | 低 |
| **W1 新增门禁（P0）** | 扩写 `check-database-framework-standard.mjs`，把 §6/§10/§11/§12 的可机器判定项变成断言（见 §6）；新增「route manifest ↔ OpenAPI ↔ handler ↔ 物理表」闭环门禁（专治 G1/E4） | ❌ | 低 |
| **W2 expand（P1）** | 新增列/新表：`uuid`、`version`、`deleted_at`、`created_by/updated_by`、`brand_id`、`barcode`、`weight_g`、`*_translation`、`commerce_price_list_item`、`commerce_product_brand`；旧列保持可写 | ✅ 仅 ADD | 中 |
| **W3 contract（P1/P2）** | 回填迁移（`price_amount`→`price_minor` 等）→ 双写校验 → 切读 → 删旧列；补 `UNIQUE`/`CHECK`（`NOT VALID` + `VALIDATE CONSTRAINT`）；relation 表归属收敛（E3 方案 A） | ✅ DROP/约束 | 高，需 owner |

---

## 6. 建议新增的门禁断言（可直接落到 `check-database-framework-standard.mjs`）

| # | 断言 | 抓哪条问题 |
|---|---|---|
| 1 | L2+ 表主键必须是 `BIGINT NOT NULL PRIMARY KEY`；出现 `id TEXT` 直接失败 | A1 |
| 2 | 时间列禁止 `TEXT`；必须是 `TIMESTAMPTZ`，或在 L0/L1 白名单内且带规范 ISO-8601 注释 | A2 |
| 3 | 金额列必须是 `*_minor BIGINT` + 同表 `currency_code CHAR(3)` + `price_scale`；出现 `price TEXT` 失败 | A4/D3 |
| 4 | `tenant_id`/`organization_id` 必须是 `BIGINT` | A3 |
| 5 | 每张业务表必须对自然键建 `UNIQUE`（或在 registry 中声明「文档化的串行化写入边界」） | A7/B5 |
| 6 | 状态类列必须有 `CHECK` 或指向枚举注册表；且注册表值集 ⊇ 代码里所有写入的字面量 | A8/A9 |
| 7 | 列表索引/查询的 `ORDER BY` 必须以上表主键或唯一键结尾 | A10 |
| 8 | JSON(B) 列不得是金额/状态/租户/权限/幂等/生命周期字段的唯一载体 | A5/C4 |
| 9 | 本地化文本列（`name`/`title`/`description`）不得出现在 base 表上（须在 `*_translation`） | F1/F3/F4 |
| 10 | **route manifest 的每条 `operationId` 必须同时存在**：① OpenAPI 条目 ② 真实 handler ③ 可解析的物理表；缺任一项失败 | G1/G2/E4 |
| 11 | 前缀注册表跨仓不得出现同一前缀的多 owner（工作区级校验） | E1 |

> 门禁本身也要自证：按 `gate-mutation-verification` 的做法，对每条新断言注入一次变异并确认它会变红，
> 否则又是一条恒绿断言。

---

## 7. 证据索引（可复核）

```bash
# 基线（6 表 / 3 索引 / 1 唯一约束 / 0 CHECK / 0 FK）
sed -n '1,124p' sdkwork-merchandise/database/ddl/baseline/postgres/0001_merchandise_baseline.sql

# 跨仓 4 张 relation 表 + cloudrouter 自述的所有权边界
sed -n '1,60p' sdkwork-cloudrouter/database/migrations/postgres/0044_product_catalog_relation_tables.up.sql

# 前缀注册表冲突
cat sdkwork-merchandise/database/contract/prefix-registry.json
cat sdkwork-cloudrouter/database/contract/prefix-registry.json

# 未实现端点
sed -n '695,737p;831,840p' sdkwork-merchandise/crates/sdkwork-merchandise-web-support/src/catalog_store.rs

# 类目 path/level_no 不重算 + 父缺失静默降级
sed -n '137,166p;1348,1374p' sdkwork-merchandise/crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog.rs

# 自造 id / 自造时间
sed -n '1272,1290p' sdkwork-merchandise/crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog.rs

# 属性 value_type/scope 硬编码
grep -n "VALUES (CAST(\$1 AS TEXT), CAST(\$2 AS TEXT), \$3, CAST(\$4 AS TEXT), \$5, 'enum', 'product'" \
  sdkwork-merchandise/crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog.rs

# 门禁假绿（实跑 EXIT=0）
node sdkwork-specs/tools/check-database-framework-standard.mjs --root sdkwork-merchandise

# i18n 覆盖
diff <(sed -n '17,40p' sdkwork-merchandise/database/seeds/locales/zh-CN/001_sku_locale.sql) \
     <(sed -n '17,40p' sdkwork-merchandise/database/seeds/locales/en-US/001_sku_locale.sql)

# sales_status 只被读、从不被 merchandise 写
grep -rn "sales_status" --include=*.rs sdkwork-merchandise sdkwork-order
```

---

## 8. 未决问题

1. **relation 表归属**（E3）：迁回 merchandise（契约与物理一致，但需 cloudrouter 改只读消费），
   还是留 cloudrouter 并在契约里显式标注 `physical_owner`？需要 database owner + cloudrouter owner 共同裁决。
2. **金额单位**：现有 `price_amount` 的「分」语义是否已在生产数据落库？只影响 W3 回填策略（若已落库需双写期）。
3. **前台/后台双类目树**（B4）是否需要？若本仓只服务 B 端（`backend/v3`），可先只做后台树，
   但需在 PRD 里明确写死这个边界，避免被当作缺陷。
4. **`commerce_product_spu.name` 与 `title`**：`name` 是死列。删除还是启用为本地化外的短名？需与消费方确认。
