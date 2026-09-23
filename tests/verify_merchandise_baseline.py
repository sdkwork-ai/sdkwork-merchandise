#!/usr/bin/env python
"""Behavioural verification of the rewritten merchandise baseline DDL and seeds.

Phase 1 applies database/ddl/baseline/postgres/0001_merchandise_baseline.sql into
a throwaway PostgreSQL schema and exercises every declared invariant with real
INSERT/UPDATE statements. Each negative case asserts the exact SQLSTATE, so a
silently-dropped constraint fails the run instead of passing it.

Phase 2 applies the baseline plus the standard seed set (resolved from
seeds/seed.manifest.json) twice, then asserts row counts, seed idempotency, and
that every stored `variant_signature` can be recomputed from the sales-axis rows
in the database.

Usage:
    python verify_merchandise_baseline.py <path-to-baseline.sql> [--repo-root <dir>]

The same script doubles as the negative control for this refactor: run it against
the pre-refactor baseline and the legacy encodings (TEXT ids, TEXT timestamps,
major-unit `*_amount` columns, no `sales_status` on the SPU) are reported as
failures instead of passing silently.
"""
from __future__ import annotations

import json
import pathlib
import sys

import psycopg
from psycopg import errors as pg_errors

DSN = "host=127.0.0.1 port=5432 user=sdkwork_ai_dev password=sdkworkdev123 dbname=sdkwork_ai_dev"
SCHEMA = "merch_baseline_check"

TABLE_COUNT_EXPECTED = 17

results: list[tuple[bool, str, str]] = []


def check(ok: bool, name: str, detail: str = "") -> None:
    results.append((ok, name, detail))
    print(("  PASS  " if ok else "  FAIL  ") + name + (f"   [{detail}]" if detail and not ok else ""))


def expect_sqlstate(conn, name, sql, params, sqlstate, expected_constraint=None):
    """Run `sql` and require it to fail with exactly `sqlstate`.

    The connection runs in autocommit mode so a rejected statement aborts only
    itself. Wrapping these probes in one transaction would roll back every
    previously inserted fixture, turning one real defect into dozens of
    misleading cascade failures.
    """
    try:
        with conn.cursor() as cur:
            cur.execute(sql, params)
        check(False, name, f"expected {sqlstate} but statement succeeded")
        return
    except pg_errors.Error as exc:
        code = getattr(exc, "sqlstate", None) or getattr(exc.diag, "sqlstate", None)
        if code != sqlstate:
            check(False, name, f"expected {sqlstate}, got {code}: {str(exc).splitlines()[0]}")
            return
        if expected_constraint:
            got = getattr(exc.diag, "constraint_name", None)
            if got != expected_constraint:
                check(False, name, f"expected constraint {expected_constraint}, got {got}")
                return
        check(True, name)


def ok(conn, name, sql, params=()):
    """Run `sql` and require it to succeed. Both outcomes are recorded, so the
    final tally is a true probe count rather than only the failures."""
    try:
        with conn.cursor() as cur:
            cur.execute(sql, params)
        check(True, name)
        return True
    except pg_errors.Error as exc:
        code = getattr(exc, "sqlstate", None)
        check(False, name, f"{code}: {str(exc).splitlines()[0]}")
        return False


def fetch_one(conn, sql, params=()):
    """Read a single row, or None when the query itself cannot run.

    Used by state-assertion checks: on a legacy schema these queries hit missing
    tables/columns, and that must be reported as a failed assertion rather than
    crashing the run (the negative control depends on it).
    """
    try:
        with conn.cursor() as cur:
            cur.execute(sql, params)
            return cur.fetchone()
    except pg_errors.Error:
        return None


def fetch_all(conn, sql, params=()):
    """Read every row, or None when the query itself cannot run."""
    try:
        with conn.cursor() as cur:
            cur.execute(sql, params)
            return cur.fetchall()
    except pg_errors.Error:
        return None


def _resolve_common(seeds_root: pathlib.Path, entry: str) -> pathlib.Path:
    """Mirror sdkwork_database_spi::seed_manifest::resolve_common_script_path.

    A bare filename resolves into `common/`; an entry whose first component is
    already `common` resolves against the seeds root. Assuming the opposite
    produces false "missing file" failures.
    """
    rel = pathlib.PurePosixPath(entry.removeprefix("./"))
    if rel.parts and rel.parts[0] == "common":
        return seeds_root / rel
    return seeds_root / "common" / rel


def _resolve_locale(seeds_root: pathlib.Path, locale: str, entry: str) -> pathlib.Path:
    """Mirror sdkwork_database_spi::seed_manifest::resolve_locale_script_path."""
    rel = pathlib.PurePosixPath(entry.removeprefix("./"))
    if len(rel.parts) >= 2 and rel.parts[0] == "locales" and rel.parts[1] == locale:
        return seeds_root / rel
    return seeds_root / "locales" / locale / rel


def seed_files(repo_root: pathlib.Path) -> list[pathlib.Path]:
    """Resolve the standard seed profile from seeds/seed.manifest.json in order."""
    seeds_root = repo_root / "database" / "seeds"
    manifest = json.loads((seeds_root / "seed.manifest.json").read_text(encoding="utf-8"))
    profile = manifest["profiles"]["standard"]
    ordered: list[pathlib.Path] = [_resolve_common(seeds_root, rel) for rel in profile["common"]]
    for locale in manifest["activeLocales"]:
        for rel in profile["locales"][locale]:
            ordered.append(_resolve_locale(seeds_root, locale, rel))
    return ordered


def verify_seed_bootstrap(conn, ddl: str, files: list[pathlib.Path]) -> None:
    """Apply baseline + seeds into a fresh schema twice and assert the result.

    Proves three things the schema alone does not: that the seeds actually run
    against the baseline in manifest order, that they are idempotent, and that
    the stored `variant_signature` agrees with the sales-axis rows taken from
    the database rather than being a hand-typed string.
    """
    with conn.cursor() as cur:
        cur.execute(f"DROP SCHEMA IF EXISTS {SCHEMA} CASCADE")
        cur.execute(f"CREATE SCHEMA {SCHEMA}")
    with conn.cursor() as cur:
        cur.execute(f"SET search_path TO {SCHEMA}")

    try:
        with conn.cursor() as cur:
            cur.execute(ddl)
    except pg_errors.Error as exc:
        check(False, "seed bootstrap: baseline applies on a clean schema", str(exc).splitlines()[0])
        return

    for path in files:
        sql = path.read_text(encoding="utf-8")
        try:
            with conn.cursor() as cur:
                cur.execute(sql)
            check(True, f"seed applies: {path.relative_to(path.parents[2])}")
        except pg_errors.Error as exc:
            check(False, f"seed applies: {path.name}", str(exc).splitlines()[0])

    expected_rows = {
        "commerce_currency": 9,
        "commerce_product_category": 1,
        "commerce_product_attribute": 2,
        "commerce_product_attribute_value": 8,
        "commerce_product_category_attribute": 2,
        "commerce_product_spu": 1,
        "commerce_product_sku": 16,
        "commerce_product_sku_attribute": 32,
        "commerce_product_spu_translation": 2,
        "commerce_product_sku_translation": 32,
        "commerce_product_attribute_translation": 2,
        "commerce_product_attribute_value_translation": 8,
    }
    for table, want in expected_rows.items():
        got = fetch_one(conn, f"SELECT COUNT(*) FROM {table}")
        got = got[0] if got else -1
        check(got == want, f"{table} has {want} seeded rows", f"got {got}")

    # The 640-yuan fact, read back through the currency registry instead of a
    # hardcoded divisor: 64000 minor / 10^2 == 640.00 CNY.
    row = fetch_one(
        conn,
        """
        SELECT s.currency_code, s.price_scale, s.sale_price_minor,
               c.minor_unit_exponent, s.list_price_minor
        FROM commerce_product_sku s
        JOIN commerce_currency c ON c.code = s.currency_code
        WHERE s.sku_no = 'basic-annual'
        """,
    )
    check(
        row == ("CNY", 2, 64000, 2, 66000),
        "seeded basic-annual is 640.00 CNY (64000 minor) with a 660.00 reference price",
        str(row),
    )
    if row:
        _, scale, sale, exponent, list_price = row
        check(
            scale == exponent and sale == 640 * 10**scale,
            "seeded price_scale agrees with the currency registry exponent",
            f"scale={scale} exponent={exponent}",
        )
        check(
            list_price is None or sale <= list_price,
            "seeded reference price is not below the charged price",
        )

    # Recompute every variant_signature from the axis rows in the database and
    # compare with the stored value. This is what makes the signature meaningful
    # rather than decorative.
    rows = fetch_all(
        conn,
        """
        SELECT s.sku_no,
               s.variant_signature,
               string_agg(a.attribute_no || '=' || av.value_code, ';' ORDER BY a.attribute_no)
        FROM commerce_product_sku s
        JOIN commerce_product_sku_attribute sa
          ON sa.sku_id = s.id AND sa.deleted_at IS NULL
        JOIN commerce_product_attribute a ON a.id = sa.attribute_id
        JOIN commerce_product_attribute_value av ON av.id = sa.attribute_value_id
        WHERE s.deleted_at IS NULL
        GROUP BY s.sku_no, s.variant_signature
        ORDER BY s.sku_no
        """,
    )
    if rows is None:
        check(False, "variant_signature is recomputable from the axis rows")
    else:
        mismatched = [(no, stored, computed) for no, stored, computed in rows if stored != computed]
        check(
            len(rows) == 16 and not mismatched,
            "every seeded variant_signature matches its sales-axis rows",
            str(mismatched[:3]),
        )
        check(
            len({computed for _, _, computed in rows}) == 16,
            "all 16 axis combinations are distinct (no collapsed SKU)",
        )

    # The two axes are declared `sales` for the category, which is what makes
    # them SKU-identifying rather than descriptive.
    sales_axes = fetch_one(
        conn,
        """
        SELECT array_agg(a.attribute_no ORDER BY a.attribute_no)
        FROM commerce_product_category_attribute ca
        JOIN commerce_product_attribute a ON a.id = ca.attribute_id
        WHERE ca.attribute_role = 'sales' AND ca.deleted_at IS NULL
        """,
    )
    check(
        sales_axes == (["period", "tier"],),
        "category declares period+tier as its sales axes",
        str(sales_axes),
    )

    # Idempotency: re-running the whole seed set must not change any count.
    for path in files:
        try:
            with conn.cursor() as cur:
                cur.execute(path.read_text(encoding="utf-8"))
        except pg_errors.Error as exc:
            check(False, f"seed is idempotent: {path.name}", str(exc).splitlines()[0])
            break
    else:
        drifted = []
        for table, want in expected_rows.items():
            got = fetch_one(conn, f"SELECT COUNT(*) FROM {table}")
            got = got[0] if got else -1
            if got != want:
                drifted.append(f"{table}={got}(want {want})")
        check(not drifted, "re-applying the seed set changes no row count", "; ".join(drifted))

    # Locale coexistence: the regression that base-column localization caused.
    locales = fetch_one(
        conn,
        """
        SELECT array_agg(DISTINCT locale ORDER BY locale)
        FROM (
            SELECT locale FROM commerce_product_sku_translation
            UNION ALL SELECT 'zh-CN' FROM commerce_product_sku WHERE id = 1600
        ) t
        """,
    )
    check(
        locales == (["en-US", "zh-CN"],),
        "en-US translation coexists with the zh-CN base value",
        str(locales),
    )

    with conn.cursor() as cur:
        cur.execute(f"DROP SCHEMA IF EXISTS {SCHEMA} CASCADE")


def main() -> int:
    ddl_path = pathlib.Path(sys.argv[1])
    ddl = ddl_path.read_text(encoding="utf-8")
    repo_root = ddl_path.resolve()
    for parent in repo_root.parents:
        if (parent / "database" / "ddl").is_dir():
            repo_root = parent
            break
    with psycopg.connect(DSN, autocommit=True) as conn:
        with conn.cursor() as cur:
            cur.execute(f"DROP SCHEMA IF EXISTS {SCHEMA} CASCADE")
            cur.execute(f"CREATE SCHEMA {SCHEMA}")
        with conn.cursor() as cur:
            cur.execute(f"SET search_path TO {SCHEMA}")

            # ---- 1. DDL applies -------------------------------------------
            try:
                cur.execute(ddl)
            except pg_errors.Error as exc:
                print(f"DDL APPLY FAILED: {getattr(exc, 'sqlstate', '?')}: {exc}")
                return 2
            cur.execute(
                "SELECT count(*) FROM information_schema.tables "
                "WHERE table_schema = %s AND table_type = 'BASE TABLE'",
                (SCHEMA,),
            )
            n = cur.fetchone()[0]
            check(
                n == TABLE_COUNT_EXPECTED,
                f"baseline creates {TABLE_COUNT_EXPECTED} tables",
                f"created {n}",
            )

            cur.execute(
                "SELECT count(*) FROM information_schema.table_constraints "
                "WHERE constraint_schema = %s AND constraint_type = 'CHECK'",
                (SCHEMA,),
            )
            n_check = cur.fetchone()[0]
            check(n_check >= 40, "baseline declares >= 40 CHECK constraints", f"found {n_check}")

            cur.execute(
                "SELECT count(*) FROM pg_indexes WHERE schemaname = %s", (SCHEMA,)
            )
            n_idx = cur.fetchone()[0]
            check(n_idx >= 35, "baseline declares >= 35 indexes", f"found {n_idx}")

        conn.autocommit = True
        with conn.cursor() as cur:
            cur.execute(f"SET search_path TO {SCHEMA}")

            # ---- 2. seed reference data -----------------------------------
            ok(
                conn,
                "currency registry accepts CNY(2) JPY(0) KWD(3) POINTS(6)",
                """
                INSERT INTO commerce_currency (id, code, minor_unit_exponent, rounding_mode, display_name) VALUES
                  (1, 'CNY', 2, 'half_up',    'Chinese Yuan'),
                  (2, 'JPY', 0, 'floor',      'Japanese Yen'),
                  (3, 'KWD', 3, 'half_up',    'Kuwaiti Dinar'),
                  (4, 'POINTS', 6, 'half_even', 'SDKWork points')
                """,
            )
            expect_sqlstate(
                conn,
                "currency rejects a bad exponent (scale > 8)",
                "INSERT INTO commerce_currency (id, code, minor_unit_exponent, display_name) VALUES (9,'XXX',9,'bad')",
                (),
                "23514",
                "ck_commerce_currency_minor_unit_exponent",
            )
            expect_sqlstate(
                conn,
                "currency rejects a lowercase / too-short code",
                "INSERT INTO commerce_currency (id, code, minor_unit_exponent, display_name) VALUES (9,'cny',2,'bad')",
                (),
                "23514",
                "ck_commerce_currency_code_shape",
            )

            ok(
                conn,
                "category root + child inserted",
                """
                INSERT INTO commerce_product_category
                  (id, tenant_id, organization_id, category_no, parent_id, path, depth, name)
                VALUES
                  (100, 100001, 0, 'membership', NULL, '/100/', 0, 'Membership'),
                  (101, 100001, 0, 'membership-annual', 100, '/100/101/', 1, 'Annual')
                """,
            )
            expect_sqlstate(
                conn,
                "category rejects a self-parent row",
                "UPDATE commerce_product_category SET parent_id = 100 WHERE id = 100",
                (),
                "23514",
                "ck_commerce_product_category_self_parent",
            )
            expect_sqlstate(
                conn,
                "category rejects a malformed materialized path",
                "INSERT INTO commerce_product_category (id,tenant_id,category_no,path,depth,name) VALUES (199,100001,'bogus','no-leading-slash',0,'Bogus')",
                (),
                "23514",
                "ck_commerce_product_category_path_shape",
            )
            expect_sqlstate(
                conn,
                "category business key is unique per tenant while live",
                "INSERT INTO commerce_product_category (id,tenant_id,category_no,name) VALUES (198,100001,'membership','Duplicate')",
                (),
                "23505",
                "uk_commerce_product_category_tenant_no",
            )

            # ---- 3. attribute tri-partition -------------------------------
            ok(
                conn,
                "attribute + values inserted (no scope column needed)",
                """
                INSERT INTO commerce_product_attribute (id, tenant_id, attribute_no, name, value_type) VALUES
                  (200, 100001, 'colour', 'Colour', 'enum'),
                  (201, 100001, 'material', 'Material', 'enum');
                INSERT INTO commerce_product_attribute_value (id, tenant_id, attribute_id, value_code, display_value) VALUES
                  (300, 100001, 200, 'red', 'Red'),
                  (301, 100001, 200, 'blue', 'Blue'),
                  (302, 100001, 201, 'oak', 'Oak');
                """,
            )
            expect_sqlstate(
                conn,
                "multi-value is rejected unless the value type is enum",
                "INSERT INTO commerce_product_attribute (id,tenant_id,attribute_no,name,value_type,is_multi_value) VALUES (299,100001,'sizes','Sizes','number',TRUE)",
                (),
                "23514",
                "ck_commerce_product_attribute_multi_value_needs_enum",
            )

            ok(
                conn,
                "same attribute gets different roles in different categories",
                """
                INSERT INTO commerce_product_category_attribute
                  (id, tenant_id, category_id, attribute_id, attribute_role, is_required, sort_order)
                VALUES
                  (400, 100001, 101, 200, 'sales',     TRUE,  10),
                  (401, 100001, 101, 201, 'parameter', TRUE,  20),
                  (402, 100001, 100, 201, 'key',       FALSE, 10)
                """,
            )
            expect_sqlstate(
                conn,
                "category attribute role is a closed set",
                "INSERT INTO commerce_product_category_attribute (id,tenant_id,category_id,attribute_id,attribute_role) VALUES (499,100001,101,201,'variant')",
                (),
                "23514",
                "ck_commerce_product_category_attribute_role",
            )
            expect_sqlstate(
                conn,
                "a category cannot bind the same attribute twice",
                "INSERT INTO commerce_product_category_attribute (id,tenant_id,category_id,attribute_id,attribute_role) VALUES (498,100001,101,200,'parameter')",
                (),
                "23505",
                "uk_commerce_product_category_attribute_binding",
            )
            # The core tri-partition claim: the SAME attribute is `parameter` in
            # 101 and `key` in 100, proving role is a property of the binding.
            rows = fetch_one(
                conn,
                """
                SELECT array_agg(attribute_role ORDER BY category_id)
                FROM commerce_product_category_attribute
                WHERE attribute_id = 201
                """,
            )
            check(
                rows == (["key", "parameter"],),
                "role is per-category: material is key in 100 and parameter in 101",
                str(rows),
            )

            # ---- 4. SPU ---------------------------------------------------
            ok(
                conn,
                "SPU inserted as draft",
                """
                INSERT INTO commerce_product_spu
                  (id, tenant_id, organization_id, spu_no, category_id, name, product_type, status)
                VALUES
                  (500, 100001, 0, 'membership-catalog', 101, 'Membership Catalog', 'membership', 'draft')
                """,
            )
            expect_sqlstate(
                conn,
                "a draft SPU cannot be sellable",
                "UPDATE commerce_product_spu SET sales_status = 'active' WHERE id = 500",
                (),
                "23514",
                "ck_commerce_product_spu_sales_requires_published",
            )
            expect_sqlstate(
                conn,
                "SPU status is a closed set",
                "UPDATE commerce_product_spu SET status = 'deleted' WHERE id = 500",
                (),
                "23514",
                "ck_commerce_product_spu_status",
            )
            ok(
                conn,
                "SPU can be activated then made sellable",
                "UPDATE commerce_product_spu SET status='active', sales_status='active', published_at=NOW() WHERE id=500",
            )
            ok(
                conn,
                "SPU key attributes stored through the relation table",
                """
                INSERT INTO commerce_product_spu_attribute
                  (id, tenant_id, spu_id, attribute_id, attribute_value_id, attribute_role) VALUES
                  (600, 100001, 500, 201, 302, 'parameter')
                """,
            )
            expect_sqlstate(
                conn,
                "spu_attribute rejects two carriers at once",
                "INSERT INTO commerce_product_spu_attribute (id,tenant_id,spu_id,attribute_id,attribute_value_id,raw_value,attribute_role) VALUES (601,100001,500,200,300,'red','parameter')",
                (),
                "23514",
                "ck_commerce_product_spu_attribute_carrier",
            )
            expect_sqlstate(
                conn,
                "spu_attribute rejects a 'sales' role (sales belong to a SKU)",
                "INSERT INTO commerce_product_spu_attribute (id,tenant_id,spu_id,attribute_id,attribute_value_id,attribute_role) VALUES (602,100001,500,200,300,'sales')",
                (),
                "23514",
                "ck_commerce_product_spu_attribute_role",
            )

            # ---- 5. SKU + the 640-yuan money fact --------------------------
            ok(
                conn,
                "SKU stores 640 CNY as 64000 minor units with declared scale",
                """
                INSERT INTO commerce_product_sku
                  (id, tenant_id, organization_id, spu_id, sku_no, variant_signature,
                   currency_code, price_scale, list_price_minor, sale_price_minor,
                   fulfillment_type, inventory_tracking, status, sales_status, published_at)
                VALUES
                  (700, 100001, 0, 500, 'basic-annual', 'colour=red',
                   'CNY', 2, 66000, 64000,
                   'membership_activation', 'none', 'active', 'active', NOW())
                """,
            )
            money = fetch_one(
                conn,
                "SELECT currency_code, price_scale, sale_price_minor FROM commerce_product_sku WHERE id = 700",
            )
            check(
                money == ("CNY", 2, 64000),
                "sale price round-trips as (CNY, scale 2, 64000) == 640.00",
                str(money),
            )
            check(
                money is not None and money[2] == 640 * 10 ** money[1],
                "stored minor units equal 640 * 10**scale (no unit drift)",
            )

            expect_sqlstate(
                conn,
                "SKU variant signature is unique per SPU",
                """
                INSERT INTO commerce_product_sku
                  (id, tenant_id, spu_id, sku_no, variant_signature, currency_code,
                   price_scale, sale_price_minor, status, sales_status)
                VALUES (701, 100001, 500, 'basic-annual-dup', 'colour=red',
                        'CNY', 2, 1, 'active', 'active')
                """,
                (),
                "23505",
                "uk_commerce_product_sku_variant",
            )
            expect_sqlstate(
                conn,
                "SKU cannot reference an unregistered currency",
                """
                INSERT INTO commerce_product_sku
                  (id, tenant_id, spu_id, sku_no, variant_signature, currency_code,
                   price_scale, sale_price_minor, status, sales_status)
                VALUES (702, 100001, 500, 'zzz-sku', 'colour=blue',
                        'ZZZ', 2, 1, 'active', 'active')
                """,
                (),
                "23503",
                "fk_commerce_product_sku_currency",
            )
            expect_sqlstate(
                conn,
                "SKU rejects a reference price below the charged price",
                "UPDATE commerce_product_sku SET list_price_minor = 50000 WHERE id = 700",
                (),
                "23514",
                "ck_commerce_product_sku_sale_not_above_list",
            )
            expect_sqlstate(
                conn,
                "SKU rejects an unknown price scale",
                "UPDATE commerce_product_sku SET price_scale = 9 WHERE id = 700",
                (),
                "23514",
                "ck_commerce_product_sku_price_scale",
            )
            expect_sqlstate(
                conn,
                "SKU rejects a negative price",
                "UPDATE commerce_product_sku SET sale_price_minor = -1, list_price_minor = NULL WHERE id = 700",
                (),
                "23514",
                "ck_commerce_product_sku_sale_price",
            )
            # Both ck_..._sales_requires_active and ck_..._published_at reject
            # "active sales + draft lifecycle". Clear published_at so exactly one
            # invariant is under test.
            expect_sqlstate(
                conn,
                "an on-sale SKU must have lifecycle status active",
                "UPDATE commerce_product_sku SET status = 'draft', published_at = NULL WHERE id = 700",
                (),
                "23514",
                "ck_commerce_product_sku_sales_requires_active",
            )
            expect_sqlstate(
                conn,
                "a published SKU cannot fall back to draft",
                "UPDATE commerce_product_sku SET status = 'draft' WHERE id = 700",
                (),
                "23514",
                "ck_commerce_product_sku_published_at",
            )
            expect_sqlstate(
                conn,
                "a non-tracked SKU cannot declare backorder",
                "UPDATE commerce_product_sku SET inventory_policy = 'backorder' WHERE id = 700",
                (),
                "23514",
                "ck_commerce_product_sku_policy_requires_tracking",
            )
            ok(
                conn,
                "a reference price may be omitted (NULL and 0 mean different things)",
                "UPDATE commerce_product_sku SET list_price_minor = NULL WHERE id = 700",
            )

            # ---- 6. SKU sales axes ---------------------------------------
            ok(
                conn,
                "SKU axis assignment inserted",
                "INSERT INTO commerce_product_sku_attribute (id,tenant_id,sku_id,attribute_id,attribute_value_id) VALUES (800,100001,700,200,300)",
            )
            expect_sqlstate(
                conn,
                "a SKU cannot carry two values on the same axis",
                "INSERT INTO commerce_product_sku_attribute (id,tenant_id,sku_id,attribute_id,attribute_value_id) VALUES (801,100001,700,200,301)",
                (),
                "23505",
                "uk_commerce_product_sku_attribute_axis",
            )

            # ---- 7. soft delete must free the business keys --------------
            ok(
                conn,
                "soft-deleting a SKU frees its sku_no and variant signature",
                "UPDATE commerce_product_sku SET deleted_at = NOW() WHERE id = 700",
            )
            ok(
                conn,
                "a new live SKU may reuse the freed sku_no",
                """
                INSERT INTO commerce_product_sku
                  (id, tenant_id, spu_id, sku_no, variant_signature, currency_code,
                   price_scale, sale_price_minor, status, sales_status)
                VALUES (703, 100001, 500, 'basic-annual', 'colour=red',
                        'CNY', 2, 64000, 'active', 'active')
                """,
            )

            # ---- 8. price list layering ----------------------------------
            ok(
                conn,
                "price list inserted",
                """
                INSERT INTO commerce_price_list (id, tenant_id, price_list_no, name, currency_code, priority)
                VALUES (900, 100001, 'cn-retail', 'China Retail', 'CNY', 100)
                """,
            )
            expect_sqlstate(
                conn,
                "price list rejects an inverted validity window",
                "UPDATE commerce_price_list SET starts_at='2026-01-02', ends_at='2026-01-01' WHERE id=900",
                (),
                "23514",
                "ck_commerce_price_list_window",
            )
            ok(
                conn,
                "tiered price item inserted",
                "INSERT INTO commerce_price_list_item (id,tenant_id,price_list_id,sku_id,currency_code,price_scale,price_minor,min_quantity) VALUES (901,100001,900,703,'CNY',2,60000,1)",
            )
            # Use a fresh quantity tier so uk_..._item_tier cannot fire first and
            # mask the currency disagreement. JPY is a registered currency, so
            # only the composite FK to the list's currency can reject this.
            expect_sqlstate(
                conn,
                "an item cannot disagree with its list's currency (composite FK)",
                "INSERT INTO commerce_price_list_item (id,tenant_id,price_list_id,sku_id,currency_code,price_scale,price_minor,min_quantity) VALUES (902,100001,900,703,'JPY',0,880,5)",
                (),
                "23503",
                "fk_commerce_price_list_item_list_currency",
            )
            expect_sqlstate(
                conn,
                "one live price per list x sku x quantity tier",
                "INSERT INTO commerce_price_list_item (id,tenant_id,price_list_id,sku_id,currency_code,price_scale,price_minor,min_quantity) VALUES (903,100001,900,703,'CNY',2,59000,1)",
                (),
                "23505",
                "uk_commerce_price_list_item_tier",
            )
            ok(
                conn,
                "a different quantity tier is allowed",
                "INSERT INTO commerce_price_list_item (id,tenant_id,price_list_id,sku_id,currency_code,price_scale,price_minor,min_quantity) VALUES (904,100001,900,703,'CNY',2,56000,10)",
            )
            expect_sqlstate(
                conn,
                "quantity tier must be a positive integer",
                "INSERT INTO commerce_price_list_item (id,tenant_id,price_list_id,sku_id,currency_code,price_scale,price_minor,min_quantity) VALUES (905,100001,900,703,'CNY',2,56000,0)",
                (),
                "23514",
                "ck_commerce_price_list_item_min_quantity",
            )

            # ---- 9. media -------------------------------------------------
            ok(
                conn,
                "product media references a stable resource id, not a url",
                "INSERT INTO commerce_product_media (id,tenant_id,owner_type,owner_id,media_role,media_resource_id,alt_text) VALUES (1000,100001,'spu',500,'main_image',777001,'Membership catalog cover')",
            )
            expect_sqlstate(
                conn,
                "only one main image per owner",
                "INSERT INTO commerce_product_media (id,tenant_id,owner_type,owner_id,media_role,media_resource_id,sort_order) VALUES (1001,100001,'spu',500,'main_image',777002,5)",
                (),
                "23505",
                "uk_commerce_product_media_main_image",
            )
            ok(
                conn,
                "gallery images coexist with the main image",
                "INSERT INTO commerce_product_media (id,tenant_id,owner_type,owner_id,media_role,media_resource_id,sort_order) VALUES (1002,100001,'spu',500,'gallery_image',777003,0)",
            )
            expect_sqlstate(
                conn,
                "media role is a closed set",
                "INSERT INTO commerce_product_media (id,tenant_id,owner_type,owner_id,media_role,media_resource_id) VALUES (1003,100001,'spu',500,'banner',777004)",
                (),
                "23514",
                "ck_commerce_product_media_role",
            )

            # ---- 10. translations: the locale-collision fix ---------------
            ok(
                conn,
                "zh-CN and en-US coexist for the same SPU field",
                """
                INSERT INTO commerce_product_spu_translation
                  (id, tenant_id, organization_id, spu_id, locale, field_name, value)
                VALUES
                  (1100, 100001, 0, 500, 'zh-CN', 'name', '会员目录'),
                  (1101, 100001, 0, 500, 'en-US', 'name', 'Membership Catalog')
                """,
            )
            tr = fetch_one(
                conn,
                """
                SELECT array_agg(locale ORDER BY locale)
                FROM commerce_product_spu_translation
                WHERE spu_id = 500 AND field_name = 'name'
                """,
            )
            check(
                tr == (["en-US", "zh-CN"],),
                "both locales survive (base-column locale seeds used to overwrite each other)",
                str(tr),
            )
            expect_sqlstate(
                conn,
                "the same locale/field cannot be stored twice",
                "INSERT INTO commerce_product_spu_translation (id,tenant_id,spu_id,locale,field_name,value) VALUES (1102,100001,500,'zh-CN','name','重复')",
                (),
                "23505",
                "uk_commerce_product_spu_translation",
            )
            expect_sqlstate(
                conn,
                "locale tags must be well formed",
                "INSERT INTO commerce_product_spu_translation (id,tenant_id,spu_id,locale,field_name,value) VALUES (1103,100001,500,'zh_CN','name','x')",
                (),
                "23514",
                "ck_commerce_product_spu_translation_locale",
            )
            expect_sqlstate(
                conn,
                "translation field names are a closed set",
                "INSERT INTO commerce_product_spu_translation (id,tenant_id,spu_id,locale,field_name,value) VALUES (1104,100001,500,'ja-JP','url','x')",
                (),
                "23514",
                "ck_commerce_product_spu_translation_field",
            )

            # ---- 11. organization sentinel --------------------------------
            expect_sqlstate(
                conn,
                "organization_id cannot be NULL (sentinel 0 is the only representation)",
                "INSERT INTO commerce_product_category (id,tenant_id,organization_id,category_no,name) VALUES (150,100001,NULL,'null-org','Null Org')",
                (),
                "23502",
            )



        # ---- 12. legacy encodings must not survive -------------------------
        # These are the checks that fail loudly on the pre-refactor baseline.
        bad_time = fetch_one(
            conn,
            """
            SELECT COUNT(*) FROM information_schema.columns
            WHERE table_schema = %s AND data_type = 'text'
              AND column_name IN ('created_at','updated_at','deleted_at','published_at','starts_at','ends_at')
            """,
            (SCHEMA,),
        )
        bad_time = bad_time[0] if bad_time else -1
        check(bad_time == 0, "no timestamp is stored as TEXT", f"{bad_time} text timestamps")

        bad_amount = fetch_one(
            conn,
            "SELECT COUNT(*) FROM information_schema.columns "
            "WHERE table_schema = %s AND column_name LIKE '%%_amount'",
            (SCHEMA,),
        )
        bad_amount = bad_amount[0] if bad_amount else -1
        check(bad_amount == 0, "no legacy *_amount column survives", f"{bad_amount} found")

        bad_major_text = fetch_one(
            conn,
            "SELECT COUNT(*) FROM information_schema.columns "
            "WHERE table_schema = %s AND column_name IN ('price_amount','original_price_amount')",
            (SCHEMA,),
        )
        bad_major_text = bad_major_text[0] if bad_major_text else -1
        check(
            bad_major_text == 0,
            "no major-unit money text column survives",
            f"{bad_major_text} found",
        )

        bad_json = fetch_one(
            conn,
            "SELECT COUNT(*) FROM information_schema.columns "
            "WHERE table_schema = %s AND data_type = 'json'",
            (SCHEMA,),
        )
        bad_json = bad_json[0] if bad_json else -1
        check(bad_json == 0, "plain JSON is not used (JSONB only)", f"{bad_json} json columns")

        bad_text_id = fetch_one(
            conn,
            """
            SELECT COUNT(*) FROM information_schema.columns
            WHERE table_schema = %s AND column_name = 'id' AND data_type <> 'bigint'
            """,
            (SCHEMA,),
        )
        bad_text_id = bad_text_id[0] if bad_text_id else -1
        check(bad_text_id == 0, "every table id is BIGINT", f"{bad_text_id} non-bigint ids")

        missing_sales_on_spu = fetch_one(
            conn,
            "SELECT COUNT(*) FROM information_schema.columns "
            "WHERE table_schema = %s AND table_name = 'commerce_product_spu' "
            "AND column_name = 'sales_status'",
            (SCHEMA,),
        )
        check(
            missing_sales_on_spu == (1,),
            "commerce_product_spu exposes sales_status (order's recharge query reads it)",
            str(missing_sales_on_spu),
        )

        with conn.cursor() as cur:
            cur.execute(f"DROP SCHEMA IF EXISTS {SCHEMA} CASCADE")

        # ---- phase 2: baseline + seeds bootstrap end to end ---------------
        if (repo_root / "database" / "seeds" / "seed.manifest.json").is_file():
            files = seed_files(repo_root)
            print(f"\n-- phase 2: baseline + {len(files)} seed file(s) --")
            verify_seed_bootstrap(conn, ddl, files)
        else:
            print(f"\n-- phase 2 skipped: no seed manifest under {repo_root} --")

    passed = sum(1 for ok_, _, _ in results if ok_)
    failed = len(results) - passed
    print(f"\n{passed} passed, {failed} failed, {len(results)} total")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
