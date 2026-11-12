import { buildProductDraft, describeProduct } from "@sdkwork/merchandise-pc-admin-core";

const productDraft = buildProductDraft({ productNo: "SPU-CATALOG-DEMO", title: "Product catalog" });

export function MerchandiseAppShell() {
  const headline = describeProduct({
    id: "catalog",
    name: productDraft.title,
    status: "draft",
  });

  return (
    <main className="merchandise-shell">
      <section className="merchandise-card">
        <h1>SDKWork Merchandise</h1>
        <p>{headline}</p>
      </section>
    </main>
  );
}
