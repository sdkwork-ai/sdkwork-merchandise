# 商品体系行业对齐回归审计（2026-11-12）

状态：已完成（implementation complete，全部仓内门禁绿）
范围：`sdkwork-merchandise` 本轮（2026-11-10 至 2026-11-12）商品体系回归检查与行业对齐
前置：[REVIEW-20260923-merchandise-catalog-model-audit.md](REVIEW-20260923-merchandise-catalog-model-audit.md)（v1 审计）、[PLAN-20260923-merchandise-catalog-model-v2.md](../plans/PLAN-20260923-merchandise-catalog-model-v2.md)（v2 方案）

---

## 0. 结论先行

对 2026-09-23 审计提出的 L1–L7 行业分层缺口做回归检查后，本轮把剩余的**全部可落地差距**闭环完毕：品牌实体、SKU 交易约束、价目表明细、SPU 参数属性写路径、属性值字典与类型化属性、单资源 retrieve、审计 actor、i18n 写路径与 locale 读回退。本轮结束时：

- 基线 19 张表（+`commerce_product_brand`、`commerce_product_brand_translation`），**每张表都有代码路径**（`catalog-table-coverage` 门禁的缺口表清零）。
- 47 个 operation / 23 条路径，全部走 SdkWorkApiResponse 信封、int64-string wire、If-Match 守卫写。
- `catalog-table-coverage` 缺口声明从 7 → **0**；`api-precondition-closure` 三层（契约/路由/SQL）守卫写计数锁在 **21**。
- 全部验收：`cargo test --workspace` 19 套件绿、`pnpm test:node` 72 项绿、spec 工具全家桶绿。

## 1. 裁决记录（本轮由 owner 做出的决定）

| 裁决 | 内容 |
|---|---|
| DDL 授权 | owner 授权本轮全部 DDL：品牌表族 + SKU 交易约束列（media 词表扩展 brand/logo 随附） |
| 价目表明细四问 | ① item 与 `sale_price_minor` 并存（两层价格）；② 解析优先级 item→sale→list 由读模型（catalog/order）执行，商品侧不算价；③ item 自带 `price_scale`（继承 list 币种，复合 FK 保证一致）；④ list 停用时 item 行保留（软删、可审计），解析以 `list.status` 为闸门 |
| i18n | 本轮打通。规则：写带可选 `locale`（缺省=默认语言写 base 行，其它 locale 写翻译行、base 行不动）；读带可选 `locale` 过滤，**缺失翻译回退基础行**（与 seed `fallbackLocale: zh-CN` 一致） |
| 条码唯一范围 | 按租户（`uk_commerce_product_spu_tenant_barcode` / `uk_commerce_product_sku_tenant_barcode` 现状）；GS1 全局唯一留给后续租户治理 |
| 组合装（bundle） | **延后并文档声明**（PRD 后续里程碑），本轮 `product_type` 五类不变 |

## 2. 行业对标矩阵（L1–L7 × 回归前后）

| 层 | 行业职责（Magento/Shopify/淘宝京东/GS1 收敛） | 2026-09-23 状态 | 2026-11-12 状态 |
|---|---|---|---|
| L1 分类字典 | 后台/前台类目树、**品牌**、属性模板 | 双树已有；**无品牌** | ✅ 双树 + `commerce_product_brand`（业务键、软删、乐观锁、logo 复用媒体机制）+ SPU `brand_id` FK；后台树 `back_parent_id` 已接线 |
| L2 属性字典 | **类型化**属性（enum/text/number/bool/date + 单位）、参数/销售二分、标准值字典 | value_type 硬编码 `enum`；无值字典 API | ✅ 类型化属性全开（valueType/inputHint/isMultiValue/unitSymbol 可设）；`attribute_values` CRUD（色卡 `colorHex`、媒体引用、软删守卫：被活跃变体引用时 409） |
| L3 类目-属性绑定 | 类目决定必填/筛选/规格轴 | 4 端点已实现 | ✅ 保持；模板继承与角色裁决不变 |
| L4 商品主数据 | SPU + 参数属性值 + 媒体 + 翻译 | **参数属性无写路径**（详情页无规格数据）；翻译无写路径 | ✅ `PUT /products/{id}/attributes` 全量替换（模板校验、key/parameter 角色、字典值 XOR raw_value）；Product 响应内嵌规格表；**i18n 双向打通**（6 翻译表） |
| L5 销售单元 | SKU + 规格组合 + barcode + 重量体积 + 成本 + 税类 + **起订量/限购/步进/包装** | barcode/重量/成本/税类为死列；**交易约束整列缺失** | ✅ 全部接线：barcode/weightGram/costPriceMinor/inventoryPolicy（backorder 需 tracked，边界 422）；**min/max_order_quantity、step_quantity、package_unit、units_per_package**（DDL CHECK + 边界校验 + 锁行决值） |
| L6 价格 | 价目表**头+明细**、阶梯/客群/区域/时间窗、多币种 | 明细有表无路径；customer_segment 死列 | ✅ `price_list_items` CRUD（阶梯 `min_quantity`、UK tier、币种从 list 继承——复合 FK 使错币种不可表达）；`customer_segment` 写/读/过滤；`currencySymbol` 读时解析 |
| L7 翻译与读模型 | 每实体的本地化 + 面向筛选的镜像 | 5 张翻译表零路径 | ✅ 6 张翻译表（含 brand）写 upsert + locale 读覆盖；搜索镜像/ C 端读模型仍归 `sdkwork-catalog`（边界不变） |

## 3. 差距闭环清单（前后对照）

| # | 差距（TECH_ARCH §9 原记录） | 闭环方式 | 证据 |
|---|---|---|---|
| 1 | 品牌实体缺失、`spu.brand_id` 死列 | W4：brand 表族 + 全链路 CRUD + FK + 过滤 | `database/ddl/baseline/postgres/0001_merchandise_baseline.sql`（19 表）、`/brands` 5 操作 |
| 2 | SPU 参数属性无写路径（「当前最大的缺口」） | W1：`products.attributes.update` 全量替换 + Product 内嵌 `attributes` | `catalog-table-coverage` 移除 `commerce_product_spu_attribute` |
| 3 | 属性类型硬编码 + 值字典无 API | W1：create 扩展 + `PATCH /attributes/{id}` + `attribute_values` CRUD | `api-request-body-closure` 9 项绿 |
| 4 | 价目表明细有表无路径（四问待决） | W2：`price_list_items` CRUD + 四问裁决落地 | `catalog-table-coverage` 移除 `commerce_price_list_item` |
| 5 | 47 个死列（商业属性/审计列/币种展示/色卡/后台树/客群） | W2/W3/W4：全部接线（详见 TECH_ARCH §9.1 更新） | `catalog-sql-baseline-closure` 列级断言绿 |
| 6 | 审计列全 NULL（created_by/updated_by/deleted_by） | W3：`IamAppContext.user_id` → 28 个写命令 → 21 条守卫语句 + 全部 INSERT/SOFT_DELETE | `tests/guarded_write.rs` 断言命令携带 actor |
| 7 | i18n 有表无路径 | W5：locale 写 upsert + 读覆盖（6 表） | `catalog-table-coverage` 缺口清零 |
| 8 | SKU 交易约束整列缺失 | W4：DDL 5 列 + CHECK + 锁行决值（min≥1、step≥1、max≥min、units≥1） | migration `0002_brand_and_trade_constraints.up.sql` |
| 9 | 单资源 retrieve 缺失（sku/category/attribute/price_list） | W3：4 个 `GET /{id}`（ETag 与 body 同版本） | `api-precondition-closure` 计数 21 |
| 10 | `inventory_policy` 死值（backorder 不可达） | W2：create/update 可设，`backorder`⇒`quantity` 双端校验 | 边界 422 消息双字段命名 |
| 11 | `CatalogRepositoryPort` 无测试替身（端到端守卫写无测试） | W3：`tests/guarded_write.rs` 内存 double（唯一性由 port-ownership 门禁保证——替身住 `tests/`） | 2 个 tokio 测试：版本往返 + 42801 |
| 12 | `tenantId/organizationId` 发布不均 | W3：Category/Product/Sku 资源补齐 | `api-response-body-closure` 属性相等断言 |

## 4. 明确不做（本轮）

- **组合装/赠品（bundle）**：owner 裁决延后；PRD 声明为后续里程碑。
- **Drive 读端口**（媒体快照存在性校验）：跨仓依赖，TECH_ARCH §9 保留声明。
- **operationId `management` 段**：属 `sdkwork-specs` 裁决，保持既有形状。
- **C 端读模型/搜索镜像/购物车**：归 `sdkwork-catalog`；**库存事实**：归 `sdkwork-inventory`；**sdkwork-order 的 `price_amount` 对齐**：归该仓。

## 5. 验证证据（2026-11-12 全绿）

```text
cargo test --workspace                     → 19/19 套件 ok
pnpm test:node                             → 72 pass / 0 fail
pnpm api:check                             → 47 operations passed
pnpm api:check:route-manifest              → 47 routes passed
pnpm api:assembly:validate                 → passed
check-api-operation-patterns               → passed
check-api-response-envelope                → passed
check-pagination                           → passed
check-rust-crate-naming-standard           → 9 crates, 0 errors
check-rust-manifest-standard               → 0 errors
check-rust-backend-composition             → passed
check-application-layering                 → passed
check-database-framework-standard          → passed（含 migration 0002 元数据）
catalog-table-coverage                     → 缺口表 0
api-precondition-closure                   → 14/14（三层守卫写计数 21）
api-request-body-closure / response        → 全绿（含品牌/交易列/locale 逐字段）
catalog-sql-baseline-closure               → 4/4（表/列/外来表/归属）
`pnpm check` / `pnpm verify`               → 环境存量失败（sdkwork-app CLI 未安装，干净 HEAD 同样失败），不计入
```

## 6. 商业化落地判定

管理面（本仓 `/backend/v3/api/catalog/*`）所依赖的商品主数据能力已完整：**品牌、类型化属性与值字典、规格落库、双层价格（含阶梯/客群）、条码/物流/税务/交易约束、多币种、软删除 + 乐观锁 + 审计 actor、多语言**。消费方（order/catalog/inventory）可按 PRD 边界接线：

- 下单可售性判定：`product_type × sales_status` sellable 投影索引 + `inventory_tracking/policy` 声明。
- 价格解析：item（市场×客群×阶梯×时间窗）→ `sale_price_minor` → `list_price_minor`，minor + scale 精确结算。
- 详情页规格：Product 内嵌 key/parameter 属性；多语言：`locale` 读参数回退基础行。

剩余跨仓事项（不阻塞本仓商业化）：`sdkwork-order` 的 money 契约对齐、`sdkwork-specs` 的 operationId 段裁决、前缀注册粒度下沉（8 仓联合变更）。
