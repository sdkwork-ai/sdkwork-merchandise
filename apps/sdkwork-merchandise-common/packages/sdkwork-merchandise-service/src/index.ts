import { isBlank } from "@sdkwork/utils";

import type {
  CreateProductInput,
  ProductProfile,
} from "@sdkwork/merchandise-contracts";

/**
 * Normalizes one create input against the authority contract: both required
 * fields are trimmed, blank business keys are refused at the boundary, and the
 * value that reaches the request is exactly what the caller stated.
 */
export function normalizeCreateProductInput(
  input: CreateProductInput,
): CreateProductInput {
  const productNo = input.productNo.trim();
  const title = input.title.trim();
  if (isBlank(productNo)) {
    throw new Error("merchandise productNo is required");
  }
  if (isBlank(title)) {
    throw new Error("merchandise title is required");
  }
  return { productNo, title };
}

export function formatProductHeadline(profile: ProductProfile): string {
  return `${profile.name} (${profile.status})`;
}
