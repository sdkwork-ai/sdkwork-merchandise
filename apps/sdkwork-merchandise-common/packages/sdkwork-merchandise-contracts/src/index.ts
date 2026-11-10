/**
 * Product lifecycle status, verbatim from the authority:
 * `ck_commerce_product_spu_status` admits exactly these four, and
 * `salesStatus` is derived from the value on every write.
 */
export type ProductStatus = "draft" | "active" | "inactive" | "archived";

/**
 * Read model of one product (SPU) row, matching the authority resource:
 * `name` is the default-locale display name the repository derives from
 * `title`; `id` rides the int64-as-string wire rule (`API_SPEC` §13.6).
 */
export interface ProductProfile {
  id: string;
  name: string;
  status: ProductStatus | string;
}

/**
 * Create input, matching the required fields of the authority
 * `CreateProductRequest`: `productNo` is the stable business key
 * (uk_commerce_product_spu_tenant_no), `title` is the default-locale display
 * title the stored `name` derives from. `categoryId` and `productType` ride on
 * the request that carries this input, which owns the closed-body contract.
 */
export interface CreateProductInput {
  productNo: string;
  title: string;
}
