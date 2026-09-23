import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

// `database/seeds/seed.manifest.json` is resolved at seed time by
// `sdkwork-database-spi`, which joins paths with rules the manifest itself does not state:
//
//   * a `common` entry whose first component is literally `common` is taken as
//     `seeds/<entry>`;
//   * any other `common` entry is taken as `seeds/common/<entry>`;
//   * a locale entry whose first two components are `locales/<locale>` is taken as
//     `seeds/<entry>`;
//   * any other locale entry is taken as `seeds/locales/<locale>/<entry>`.
//
// Because that resolution happens at runtime, a typo in this manifest is discovered by a failing
// `db:seed` against a live database rather than by `pnpm check`. These tests move the failure back
// to the file that owns the paths, and close two further holes: a declared locale checksum that
// nobody recomputes, and a seed script that is shipped but referenced by no profile.

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const seedsDir = path.join(root, "database/seeds");
const manifestPath = path.join(seedsDir, "seed.manifest.json");

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

function normalise(entry) {
  return entry.startsWith("./") ? entry.slice(2) : entry;
}

/** Mirrors `resolve_common_script_path` in `sdkwork-database-spi/src/seed_manifest.rs`. */
function resolveCommonScript(entry) {
  const relative = normalise(entry);
  const first = relative.split("/")[0];
  return first === "common" ? path.join(seedsDir, relative) : path.join(seedsDir, "common", relative);
}

/** Mirrors `resolve_locale_script_path` in `sdkwork-database-spi/src/seed_manifest.rs`. */
function resolveLocaleScript(locale, entry) {
  const relative = normalise(entry);
  const [first, second] = relative.split("/");
  return first === "locales" && second === locale
    ? path.join(seedsDir, relative)
    : path.join(seedsDir, "locales", locale, relative);
}

/**
 * The declared checksum of a locale set.
 *
 * The rule is `sha256` over the **LF-normalised UTF-8 text** of the set's files, concatenated in
 * declared order. LF normalisation is not cosmetic: the checkout convention is CRLF
 * (`core.autocrlf=true`), so hashing the bytes on disk would make the same file hash differently on
 * a Windows checkout and on a Linux one, and the value would be wrong for whichever platform did not
 * produce it. Normalising first makes the digest a property of the content rather than of the
 * checkout.
 *
 * Reads as UTF-8 text, not raw bytes, for the same reason.
 */
function localeSetChecksum(files) {
  const normalised = files
    .map((file) => readFileSync(path.join(seedsDir, file), "utf8").replace(/\r\n/g, "\n"))
    .join("");
  return `sha256:${createHash("sha256").update(normalised, "utf8").digest("hex")}`;
}

function listSqlFiles(directory) {
  if (!existsSync(directory)) return [];
  const found = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      found.push(...listSqlFiles(full));
    } else if (entry.isFile() && entry.name.endsWith(".sql")) {
      found.push(full);
    }
  }
  return found;
}

test("every seed script named by every profile resolves to a file on disk", () => {
  const problems = [];
  let checked = 0;

  for (const [profileName, profile] of Object.entries(manifest.profiles ?? {})) {
    for (const entry of profile.common ?? []) {
      checked += 1;
      const resolved = resolveCommonScript(entry);
      if (!existsSync(resolved) || !statSync(resolved).isFile()) {
        problems.push(
          `profiles.${profileName}.common entry \`${entry}\` resolves to ${path.relative(root, resolved)}, which does not exist`,
        );
      }
    }
    for (const [locale, entries] of Object.entries(profile.locales ?? {})) {
      for (const entry of entries) {
        checked += 1;
        const resolved = resolveLocaleScript(locale, entry);
        if (!existsSync(resolved) || !statSync(resolved).isFile()) {
          problems.push(
            `profiles.${profileName}.locales.${locale} entry \`${entry}\` resolves to ${path.relative(root, resolved)}, which does not exist`,
          );
        }
      }
    }
  }

  assert.ok(checked > 0, "the manifest must declare at least one seed script");
  assert.deepEqual(problems, [], problems.join("\n"));
});

test("every localeSet file exists and its declared checksum is the real one", () => {
  const problems = [];

  for (const [locale, set] of Object.entries(manifest.localeSets ?? {})) {
    for (const file of set.files ?? []) {
      if (!existsSync(path.join(seedsDir, file))) {
        problems.push(`localeSets.${locale}.files lists \`${file}\`, which does not exist`);
      }
    }

    const present = (set.files ?? []).filter((file) => existsSync(path.join(seedsDir, file)));
    if (present.length !== (set.files ?? []).length) continue;
    if (!set.checksum) {
      if (set.required) {
        problems.push(`localeSets.${locale} is required but declares no checksum`);
      }
      continue;
    }

    const actual = localeSetChecksum(set.files);
    if (set.checksum !== actual) {
      problems.push(
        `localeSets.${locale}.checksum is \`${set.checksum}\` but the declared files hash to \`${actual}\``,
      );
    }
  }

  assert.deepEqual(problems, [], problems.join("\n"));
});

test("a locale set is declared exactly once, and every locale key is a supported locale", () => {
  const supported = new Set(manifest.supportedLocales ?? []);
  const problems = [];

  for (const locale of Object.keys(manifest.localeSets ?? {})) {
    if (!supported.has(locale)) {
      problems.push(`localeSets key \`${locale}\` is not a member of supportedLocales`);
    }
  }

  // `profiles.*.locales` and `localeSets` both name locale seed files. A locale present in one and
  // absent from the other means the profile plan and the integrity-checked set disagree about what
  // that locale ships.
  for (const [profileName, profile] of Object.entries(manifest.profiles ?? {})) {
    for (const locale of Object.keys(profile.locales ?? {})) {
      if (!(locale in (manifest.localeSets ?? {}))) {
        problems.push(
          `profiles.${profileName}.locales declares \`${locale}\` but localeSets has no such key`,
        );
      }
    }
  }

  for (const [locale, set] of Object.entries(manifest.localeSets ?? {})) {
    if (!(set.files ?? []).length) continue;
    const declaredBySomeProfile = Object.values(manifest.profiles ?? {}).some((profile) =>
      Object.prototype.hasOwnProperty.call(profile.locales ?? {}, locale),
    );
    if (!declaredBySomeProfile) {
      problems.push(
        `localeSets.${locale} ships ${set.files.length} file(s) but no profile.locales entry runs them`,
      );
    }
  }

  assert.deepEqual(problems, [], problems.join("\n"));
});

test("no seed script is shipped without a profile that runs it", () => {
  const referenced = new Set();
  for (const profile of Object.values(manifest.profiles ?? {})) {
    for (const entry of profile.common ?? []) {
      referenced.add(resolveCommonScript(entry));
    }
    for (const [locale, entries] of Object.entries(profile.locales ?? {})) {
      for (const entry of entries) {
        referenced.add(resolveLocaleScript(locale, entry));
      }
    }
  }

  const orphans = [...listSqlFiles(path.join(seedsDir, "common")), ...listSqlFiles(path.join(seedsDir, "locales"))]
    .filter((file) => !referenced.has(file))
    .map((file) => path.relative(root, file));

  assert.deepEqual(
    orphans,
    [],
    `seed scripts exist but no seed profile runs them:\n${orphans.join("\n")}`,
  );
});
