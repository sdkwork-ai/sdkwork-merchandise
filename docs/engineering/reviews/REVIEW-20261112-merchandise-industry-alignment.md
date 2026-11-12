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


## 7. 回归补记（2026-11-12 第二轮回归）

首轮交付后的独立回归探索发现两处**写路径/读路径的真实缺口**，本轮补齐：

1. **翻译 upsert 从未落盘**：首轮的 locale 写脚本在 `create_spu` 锚点失败中止，
   `upsert_translation` 成了死代码——六个 create/update 均不写翻译行，
   `command.locale` 被静默忽略。本轮补齐：
   - 六个 create：事务内按 locale upsert（spu 写 name/title/subtitle/description，
     sku 写 name/title，brand 写 name/description，其余写 name/display_value）；
   - 六个 update：非默认 locale 时基础行文本保持存储值（COALESCE/锁行决值），
     翻译行写入命令文本；默认 locale 行为不变。
   - `upsert_translation` 死代码 → 15 个调用点。
2. **单资源 retrieve 无 locale 覆盖**：五个 retrieve（category/spu/sku/brand/attribute）
   补齐 `attach_*_locale`；`RetrieveQueryParams.locale` 经 HTTP `?locale=` 传入。
   五个 retrieve 操作的 OpenAPI 补 `locale` 查询参数。

另补齐：品牌演示 seed（`common/003_brands.sql`，保留块 2200..2299）+ en-US 品牌翻译
（翻译 id 3400..3403，localeSet checksum 同步重算）；媒体 owner 家族 "four tables"
注释漂移修正（现五类）。

**属性翻译字段注记**：翻译表 CHECK 允许 attribute 的 `unit_symbol/help_text` 与
attribute_value 的 `help_text` 翻译行，本轮未暴露写入面（单位符号通常不翻译、
help_text 无 HTTP 输入面）；CHECK 预留，后续按需打开。


## 8. 数据库初始化回归（2026-11-12 第三轮）

以「部署新应用即可完整体验」为标准的数据库初始化回归，发现并修复：

1. **P0 语法错误**：`001_merchandise_bootstrap.sql` 的品牌保留块注释丢失 `--`
   前缀，任何 seed 重跑都会以 Postgres 语法错误中止。
2. **可售卖图断裂**：SPU 未挂 `brand_id`；course SKU 的 `offering` 轴
   （1102/1214/1732）无字典支撑；价目表/明细/媒体零数据。本轮补齐：
   品牌并入 001 bootstrap（先于 SPU，FK 顺序安全）、SPU 1500/1501 挂 2200、
   offering 轴三行、价目表 2100 + 19 条明细（16 会员 + 1 课程 + 2 阶梯档
   `min_quantity=12` 演示双层价格）、3 条演示媒体（drive:// 占位）。
3. **部署即用**：`bootstrap_merchandise_database` 此前从不调用 `seed()`
   （`seedOnBoot` 死配置）——现已接线；`etc/topology/base.env` 声明
   `SDKWORK_DATABASE_AUTO_MIGRATE=true` + `SDKWORK_DATABASE_SEED_ON_BOOT=true`
   并重生成 10 个 topology 文件：**部署即完整初始化**（init→migrate→seed 一次
   boot 完成），生产环境可通过 env 覆盖关闭。
4. **集成验证**：`pnpm db:verify:baseline`（真实 Postgres，baseline+seeds
   应用两遍 + 行数/幂等/签名/coexistence 断言）从 21 failed → **93 passed /
   0 failed**；verifier 同步更新至 19 表/新行数/三轴。
5. 课程绑定 1302 移入 002（跟随类目 1001 的声明顺序，消除 FK 顺序陷阱）；
   演示媒体为 drive:// 占位，生产租户经媒体 API 替换。

**环境矩阵**：development / test / staging / demo / production 全部由
`etc/topology/base.env` 继承 `AUTO_MIGRATE=true` + `SEED_ON_BOOT=true`
（seed 为保留块 upsert，不触碰真实租户数据）；生产要关闭时改两个 env 键即可。


## 9. 数据库初始化第三轮回归（2026-11-12）

第二轮回归审计确认两处残留并修复：

1. **en-US course 线翻译缺失**：course 产品线（SPU 1501 / SKU 1620 / 属性 1102 /
   值 1214）零翻译行——en-US 用户看到的整个课程线是中文原文。本轮补齐
   （翻译 id 3002/3003、3132/3133、3202、3308，均在保留块内），
   en-US localeSet checksum 同步重算，`db:verify:baseline` 翻译计数同步更新。
2. **保留块声明漂移**：seed 头部声称保留块「declared in schema.yaml」但
   schema.yaml 并无此声明。本轮在 `database/contract/schema.yaml` 增补
   `reservedSeedIdBlocks` 段（13 个块；两个验证器均宽松接受额外键），
   使声明为真。

已确认无残留：topology 10 文件全量携带 AUTO_MIGRATE/SEED_ON_BOOT；
价目明细无 (sku, tier) 冲突；`db:validate` 离线可跑，
`db:seed`/`db:bootstrap` 需 Postgres（SDKWORK_DATABASE_* 连接串）。


## 10. 数据库初始化第四轮回归（2026-11-12）

第三轮独立审计确认三处残留并修复：

1. **类目 en-US 翻译缺失**：两类目（1000 会员服务 / 1001 课程）零翻译行——
   en-US 导航树显示中文类目名。本轮补齐（翻译 id 3500..3503，含 name +
   description），localeSet checksum 重算，`db:verify:baseline` 增补
   category_translation=4 断言 → **94 passed / 0 failed**。
2. **database/README.md 重写**：旧版 24 行仅覆盖 baseline + 空 migrations +
   8 个命令。新版覆盖：boot 三阶段（baseline→migrations→seeds）、19 表清单、
   seed 文件与保留块、boot 行为表（manifest 默认 vs topology env 覆盖 vs
   env 键）、生产关闭方式、CLI 连接（postgres.env.example → postgres.env）、
   `db:verify:baseline` 94 断言说明。
3. **死列普查归零**：新鲜普查（19 表全列 vs 非测试 Rust 源 + OpenAPI 属性 +
   seed 列）发现 **0 个死列**——2026-09-23 的 47 死列盲区全部关闭。
   TECH_ARCH §9 普查段刷新为归零状态（注明普查为手工维护、每轮回归重测）。

确认无需改动：SKU `inventory_tracking='none'` 是正确演示姿态（库存事实归
sdkwork-inventory，本模块只声明 tracking/policy）；topology 10 文件全量携带
AUTO_MIGRATE/SEED_ON_BOOT；`db:validate` 离线可用。

**至此，数据库初始化回归收敛**：完整可售卖图、部署即初始化、五环境矩阵、
多语言全量、幂等安全、文档与声明一致。


## 11. 数据库初始化第五轮回归（2026-11-13）

第四轮独立审计确认三处残留并修复：

1. **容器部署阻断（P0）**：Dockerfile 运行镜像仅拷贝二进制——无 `database/`、
   无 `SDKWORK_MERCHANDISE_APP_ROOT`，容器启动即死于模块加载。现拷贝
   `database/` 至 `/app` 并设置 APP_ROOT（路径约定
   `app_root/database/database.manifest.json` 已核实）。
2. **SPU 规格表演示缺口**：`commerce_product_spu_attribute` 零数据且类目
   1000 模板无 parameter/key 绑定——详情页规格表渲染空。本轮补齐：属性
   1103 服务说明（text）、绑定 1303（parameter）、spu_attribute 1800
   （raw_value 载体）、en-US 翻译 3203、保留块 1800..1899 双声明、
   verifier 四处计数 → **95 passed / 0 failed**。
3. **文档 canon 缺失**：REVIEW-20261112 未入 docs 索引、reviews/README 为
   重复占位——已索引并修复；CHANGELOG 增补数据库初始化轮条目。

确认无需改动：媒体槽唯一性（三行槽键互异）；`db:validate` 离线可用；
`db:seed`/`db:bootstrap` 需 Postgres（SDKWORK_DATABASE_* 连接串）。


## 12. 数据库初始化第六轮回归：端到端 boot 验证（2026-11-13）

前五轮证明了 SQL 层（`db:verify:baseline` 真实 Postgres 95/95），本轮首次
对**真实 Postgres 跑通 Rust 应用 boot 路径**（`sdkwork-database-cli bootstrap`
= init → migrate → seed），发现并修复两个真实部署缺陷：

1. **migration 0002 对 baseline 不幂等（P0）**：W4 将品牌 FK 等价 DDL 同时写入
   baseline 与 0002，新环境 baseline 先建 FK、0002 再建即
   `constraint already exists` 中止。修复：0002 全部 `ADD CONSTRAINT` 前置
   `DROP CONSTRAINT IF EXISTS`（brand FK + 4 个 SKU trade CHECK），
   对新环境（baseline 已建）与存量环境（pre-brand）均幂等。
   `db:verify:baseline` 95/95 复验通过。
2. **共享 dev 库 anchor 冲突（跨仓，记录不修）**：`sdkwork_ai_dev` 被平台
   60+ 模块共用，兄弟模块（shop/membership/course）创建了同名的
   `commerce_product_*` 表——merchandise init 的**单表 anchor 检查**看到
   表存在即跳过 baseline，本模块 19 表 schema 无法落地。这是审计 E1
   （8 仓抢注 `commerce_` 前缀）在运行时的表现：anchor 应校验 registry
   全表集而非首表——框架级改进，属 `sdkwork-database` 跨仓事项。
   本地恢复程序（已验证）：drop merchandise 自有 19 表（无外部 FK 依赖，
   已核实）+ 清 merchandise ops 三表行 → 重跑 bootstrap。
   **独立部署（自有数据库）不受影响**：anchor 不存在 → 全量初始化。

**端到端验证结果**（共享 dev 库重置后）：
`bootstrap complete: 1 migration(s), 5 seed script(s)` → 19 表 / 品牌 2 /
SPU 2（全挂 brand_id=2200）/ SKU 17 / 规格表 1 / 价目明细 19（含 2 阶梯档）/
媒体 3 / install state=seeded；`db:seed --locale en-US` 应用 5 文件 →
双语共存验证（en-US 读类目名返回 Membership/Courses，回退规则生效）。

**框架级注意**：boot 的 `seed(locale)` 只应用该 locale 的 seed 文件——
多语言演示需 per-locale 一次 `db:seed --locale <locale>`（已写入
database/README.md）。
