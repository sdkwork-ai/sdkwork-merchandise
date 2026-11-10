import { describe, expect, it } from "vitest";

import { formatProductHeadline, normalizeCreateProductInput } from "../src/index.ts";

describe("merchandise service helpers", () => {
  it("normalizes the authority create input and refuses blank business keys", () => {
    expect(
      normalizeCreateProductInput({ productNo: " spu-demo-01 ", title: " Demo Merchandise " }),
    ).toEqual({
      productNo: "spu-demo-01",
      title: "Demo Merchandise",
    });
    expect(() => normalizeCreateProductInput({ productNo: "  ", title: "Demo" })).toThrow(
      "merchandise productNo is required",
    );
    expect(() => normalizeCreateProductInput({ productNo: "spu-demo-01", title: " " })).toThrow(
      "merchandise title is required",
    );
  });

  it("formats merchandise headline", () => {
    expect(formatProductHeadline({ id: "1", name: "Demo", status: "draft" })).toBe(
      "Demo (draft)",
    );
  });
});
