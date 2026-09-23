// Port-implementation closure gate for the merchandise backend.
//
// A component specification can advertise a port that no code implements, and every existing gate
// stays green. `check-component-port-bindings` validates the *shape* of the declaration — that
// `name` and `export` are present, that `export` is one of `contracts.publicExports`, and that a
// required port has some provider somewhere — but it never looks for `impl <Port> for <Type>`.
// Nothing else reads `contracts` at all. The catalog repository port sat in exactly that blind
// spot: advertised by `crates/sdkwork-merchandise-service/specs/component.spec.json`, materialised
// into `generated/composition.resolved.json`, and resolvable by the composition resolver for
// several rounds while `impl CatalogRepositoryPort` did not exist anywhere in the workspace.
//
// The blind spot has a second half. `APPLICATION_LAYERED_ARCHITECTURE_SPEC.md` forbids a route
// crate to depend on a concrete repository crate, but the shared Rust composition validator applies
// that rule only to *service* crates — a route crate could take a `*-repository-sqlx` dependency
// and no gate would notice. That is how the port ended up declared in the HTTP adapter, which
// depended on the SQL repository to name the type it implemented the port for.
//
// This gate closes both halves against the committed sources, so the defect cannot return through a
// later refactor without turning a test red.
//
// # Non-vacuity
//
// Eight mutations were injected one at a time and every one was caught. The set of failing
// assertion numbers is listed so the battery can be repeated after any change to this file:
//
// | mutation                                                  | red assertions |
// | --------------------------------------------------------- | -------------- |
// | the port implementation is renamed away                    | 4, 5           |
// | the only implementation survives as a comment              | 4, 5           |
// | the advertised port target points at another crate         | 3, 4, 5, 8     |
// | the advertised port names no item at all                   | 1, 2, 4, 5, 8  |
// | the trait is renamed, so the advertised item is gone       | 3, 4, 5        |
// | the HTTP adapter takes a concrete repository dependency    | 6, 7           |
// | the route crate takes a concrete repository dependency     | 6, 7           |
// | the committed resolution advertises an undeclared port     | 8              |
//
// The second row is the one that matters most: an earlier draft of this gate read raw source, and
// the service crate's own documentation quotes the sentence it was looking for, so the gate stayed
// green with the real binding commented out. `stripComments` below is the fix, and that mutation is
// how the fix is proven rather than assumed.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { test } from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

/** The working tree checks files out as CRLF; every pattern below is written for LF. */
function readText(relativePath) {
  return readFileSync(path.join(root, relativePath), 'utf8').replace(/\r\n/g, '\n');
}

function exists(relativePath) {
  try {
    statSync(path.join(root, relativePath));
    return true;
  } catch {
    return false;
  }
}

// ------------------------------------------------------------------- inventory

/** Every directory under `crates/`, sorted for deterministic failure messages. */
const crateDirs = readdirSync(path.join(root, 'crates'))
  .filter((entry) => statSync(path.join(root, 'crates', entry)).isDirectory())
  .sort();

/**
 * Removes Rust comments before any declaration or implementation is searched for.
 *
 * Prose is not evidence. The service crate's own port documentation quotes the sentence
 * ``impl CatalogRepositoryPort for PostgresCommerceCatalogStore`` while explaining where the
 * binding lives, and a scan over raw text reads that sentence as an implementation — the gate would
 * then pass on the strength of the comment that describes the fix rather than on the fix, and would
 * keep passing after the real binding was deleted.
 */
function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/[^\n]*/g, ' ');
}

/** Reads every `.rs` file under a crate's `src/`, concatenated, with comments removed. */
function crateSource(crateDir) {
  const srcRoot = path.join(root, 'crates', crateDir, 'src');
  const chunks = [];
  const walk = (absolute) => {
    for (const entry of readdirSync(absolute).sort()) {
      const child = path.join(absolute, entry);
      if (statSync(child).isDirectory()) walk(child);
      else if (entry.endsWith('.rs')) {
        chunks.push(stripComments(readFileSync(child, 'utf8').replace(/\r\n/g, '\n')));
      }
    }
  };
  walk(srcRoot);
  return chunks.join('\n');
}

const DEP_SECTIONS = new Set(['dependencies', 'dev-dependencies', 'build-dependencies']);

/**
 * A crate's manifest facts.
 *
 * Dependencies are read section by section rather than by a bare `key = ...` scan: `[lints]`,
 * `[package]`, and `[lib]` all carry `key = value` lines, and treating those as dependencies would
 * invent edges that do not exist. `normal` keeps only `[dependencies]`, because a dependency the
 * test harness alone needs cannot let production code bypass a port.
 */
function parseCargo(crateDir) {
  const text = readText(`crates/${crateDir}/Cargo.toml`);
  const packageName = /^name = "([^"]+)"/m.exec(text)?.[1] ?? null;
  const libSection = text.slice(text.indexOf('[lib]'));
  const libName = /^name = "([^"]+)"/m.exec(libSection)?.[1] ?? packageName;
  const dependencies = [];
  const normal = [];
  let section = null;
  for (const line of text.split('\n')) {
    const heading = /^\[([^\]]+)\]/.exec(line);
    if (heading) {
      section = heading[1];
      continue;
    }
    if (!DEP_SECTIONS.has(section)) continue;
    // Two spellings have to be recognised: `axum = { workspace = true }` and the inherited form
    // `sdkwork_merchandise_repository_sqlx.workspace = true`. A pattern that only handled the first
    // would silently see no sibling dependencies at all, and every dependency rule below would pass
    // vacuously — which is exactly the failure mode this gate exists to prevent.
    const dep = /^([A-Za-z0-9_-]+)(?:\.workspace)?\s*=/.exec(line);
    if (!dep) continue;
    // Cargo dependency names are matched with underscores normalised to dashes, the same way the
    // shared Rust composition validator normalises them.
    const name = dep[1].replaceAll('_', '-');
    dependencies.push(name);
    if (section === 'dependencies') normal.push(name);
  }
  return { crateDir, packageName, libName, dependencies, normal, source: crateSource(crateDir) };
}

const crates = crateDirs.map(parseCargo);
const crateByPackage = new Map(crates.map((crate) => [crate.packageName, crate]));
const crateByLibName = new Map(crates.map((crate) => [crate.libName, crate]));

/** Every `specs/component.spec.json` in the workspace, with its parsed contracts. */
function listComponentSpecs() {
  const specs = [];
  const walk = (absoluteDir) => {
    for (const entry of readdirSync(absoluteDir).sort()) {
      if (entry === 'node_modules' || entry === 'target' || entry.startsWith('.')) continue;
      const child = path.join(absoluteDir, entry);
      if (statSync(child).isDirectory()) walk(child);
      else if (entry === 'component.spec.json') {
        specs.push({
          relativePath: path.relative(root, child).replaceAll('\\', '/'),
          spec: JSON.parse(readFileSync(child, 'utf8').replace(/\r\n/g, '\n')),
        });
      }
    }
  };
  walk(root);
  return specs;
}

const componentSpecs = listComponentSpecs();
/**
 * The specs that describe a crate under `crates/`.
 *
 * The repository root also carries a component spec, and it is a different kind of document: it
 * describes the workspace, whose `provides` entry is a runtime handle rather than a Rust item, so
 * it names no `target`. Reading a port declaration as a crate-level claim is only meaningful where
 * a crate exists to hold it, which is why the closure rules below are scoped to `crates/`.
 */
const crateSpecs = componentSpecs.filter((entry) =>
  String(entry.spec.component?.root ?? '')
    .replaceAll('\\', '/')
    .startsWith('crates/'),
);
/** A spec is Rust-owned when its component declares Rust among its languages. */
const rustSpecs = crateSpecs.filter((entry) => entry.spec.component?.languages?.includes('rust'));

// ---------------------------------------------------------- provided ports

/** Every provided port that names a concrete item, and the ones that name nothing. */
const boundPorts = [];
const unboundPorts = [];
for (const entry of rustSpecs) {
  for (const port of entry.spec.contracts?.providedPorts ?? []) {
    if (typeof port === 'string' || !port.export) continue;
    const target = typeof port.target === 'string' ? port.target : null;
    if (!target) {
      unboundPorts.push(`${entry.relativePath}: provided port ${port.name} names no target`);
      continue;
    }
    const [cratePath, item] = target.split('::');
    boundPorts.push({
      specPath: entry.relativePath,
      component: entry.spec.component.name,
      name: port.name,
      cratePath,
      item,
      target,
    });
  }
}

/**
 * The item kinds a `target` may legitimately point at.
 *
 * `catalogServiceContract` resolves to a value (`pub fn`), `CatalogRepositoryPort` to a trait, and
 * `catalogRepository` to a struct. The gate does not need to know which is which in advance — it
 * reads the declaration back out of the crate source, which is what makes the check two-sided: the
 * specification and the crate must agree, in both spelling and kind.
 */
const ITEM_KINDS = ['trait', 'struct', 'enum', 'fn', 'type', 'const'];

function declaredKind(source, item) {
  for (const kind of ITEM_KINDS) {
    if (new RegExp(`\\bpub ${kind} ${item}\\b`).test(source)) return kind;
  }
  return null;
}

/** `impl <Trait> for <Type>` sites across the whole workspace, as `{ crate, type }`. */
function implementationsOf(item) {
  const pattern = new RegExp(`\\bimpl(?:<[^>]*>)?\\s+${item}\\s+for\\s+([A-Za-z0-9_:]+)`, 'g');
  const found = [];
  for (const crate of crates) {
    for (const match of crate.source.matchAll(pattern)) {
      found.push({ crate: crate.packageName, type: match[1] });
    }
  }
  return found;
}

// --------------------------------------------------------- dependency direction

/**
 * Package-name shapes the layering specification allows to construct dependencies.
 *
 * `APPLICATION_LAYERED_ARCHITECTURE_SPEC.md` section 4.1 assigns `sdkwork-<code>-service-host`,
 * `-native-host`, `-tauri-host`, `sdkwork-api-<code>-assembly`, and the gateways to L5, and says of
 * them: "Gateway and service-host crates construct and mount dependencies." Those, and a spec that
 * declares a runtime composition role, are the crates permitted to name a concrete repository.
 */
const COMPOSITION_ROOT_NAME_PATTERNS = [
  /-service-host$/,
  /-native-host$/,
  /-tauri-host$/,
  /-standalone-gateway$/,
  /-cloud-gateway$/,
  /-api-server$/,
  /^sdkwork-api-.+-assembly$/,
];

const COMPOSITION_ROOT_LAYER_ROLES = new Set([
  'runtime-composition',
  'runtime-gateway',
  'runtime-host',
]);

function isCompositionRoot(crate) {
  const spec = componentSpecs.find((entry) => entry.spec.component?.name === crate.packageName);
  const layerRole = spec?.spec?.contracts?.layerRole;
  if (layerRole && COMPOSITION_ROOT_LAYER_ROLES.has(layerRole)) return true;
  return COMPOSITION_ROOT_NAME_PATTERNS.some((pattern) => pattern.test(crate.packageName ?? ''));
}

const REPOSITORY_DEPENDENCY = /-repository-/;

// ================================================================ assertions

test('the component specs, the crate manifests, and the committed resolution all parse', () => {
  assert.ok(rustSpecs.length >= 6, `expected the Rust component specs, saw ${rustSpecs.length}`);
  assert.ok(crateDirs.length >= 9, `expected the workspace crates, saw ${crateDirs.length}`);
  assert.ok(crateByLibName.has('sdkwork_merchandise_service'), 'the service crate must be indexed by lib name');
  assert.ok(
    componentSpecs.some((entry) => entry.relativePath === 'specs/component.spec.json'),
    'the workspace root component spec must be found',
  );
  const resolution = JSON.parse(readText('generated/composition.resolved.json'));
  assert.ok(resolution.architecture?.components?.length >= 8, 'the committed resolution must list the components');
  assert.ok(boundPorts.length >= 3, `expected the bound provided ports, saw ${boundPorts.length}`);
});

test('no Rust provided port is a declaration that names nothing', () => {
  assert.deepEqual(
    unboundPorts,
    [],
    'a Rust crate that provides a port must say which item it provides; a port with no target cannot be implemented or consumed',
  );
});

test('every provided port target is an item the declaring crate really declares', () => {
  const problems = [];
  for (const port of boundPorts) {
    const crate = crateByLibName.get(port.cratePath);
    if (!crate) {
      problems.push(`${port.specPath}: port ${port.name} targets unknown crate path ${port.cratePath}`);
      continue;
    }
    if (crate.packageName !== port.component) {
      problems.push(
        `${port.specPath}: port ${port.name} targets ${port.cratePath}, which is ${crate.packageName}, not the declaring component`,
      );
      continue;
    }
    const kind = declaredKind(crate.source, port.item);
    if (!kind) {
      problems.push(
        `${port.specPath}: port ${port.name} advertises ${port.target}, but crates/${crate.crateDir}/src declares no such item`,
      );
    }
  }
  assert.deepEqual(problems, []);
});

test('every provided trait port has at least one real implementation in the workspace', () => {
  const problems = [];
  const report = [];
  for (const port of boundPorts) {
    const crate = crateByLibName.get(port.cratePath);
    if (!crate || declaredKind(crate.source, port.item) !== 'trait') continue;
    const implementors = implementationsOf(port.item);
    if (implementors.length === 0) {
      problems.push(
        `${port.specPath}: port ${port.name} declares trait ${port.target} and nothing implements it; a declared port with no implementation resolves in composition and fails at runtime`,
      );
      continue;
    }
    report.push(`${port.item} <- ${implementors.map((entry) => `${entry.crate}::${entry.type}`).join(', ')}`);
  }
  assert.deepEqual(problems, [], 'a trait port that no crate implements is not a port, it is a promise');
  assert.ok(report.length >= 1, `expected at least one trait port to be checked, saw ${report.length}`);
});

test('the catalog repository port is owned by the service crate and bound by the repository crate', () => {
  const port = boundPorts.find((entry) => entry.item === 'CatalogRepositoryPort');
  assert.ok(port, 'the service crate must keep advertising CatalogRepositoryPort');
  assert.equal(port.target, 'sdkwork_merchandise_service::CatalogRepositoryPort');
  assert.equal(declaredKind(crateByLibName.get(port.cratePath).source, port.item), 'trait');

  const implementors = implementationsOf('CatalogRepositoryPort');
  assert.deepEqual(
    implementors.map((entry) => entry.crate),
    ['sdkwork-merchandise-repository-sqlx'],
    'the port must be implemented by the repository crate and by nothing else',
  );
  assert.equal(implementors[0].type, 'PostgresCommerceCatalogStore');

  // The binding must be findable, not merely present: the file is the audit point for this port.
  assert.ok(
    exists('crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog_port.rs'),
    'the port binding must live in its own repository-crate module',
  );
});

test('no crate outside a composition root depends on a concrete repository crate', () => {
  const violations = [];
  for (const crate of crates) {
    // A repository crate may depend on another repository crate: sharing row mapping is not an
    // inversion, and the layering rule is about who is allowed to *construct*, not to compose.
    if (REPOSITORY_DEPENDENCY.test(crate.packageName ?? '')) continue;
    if (isCompositionRoot(crate)) continue;
    for (const dep of crate.normal) {
      if (REPOSITORY_DEPENDENCY.test(dep)) {
        violations.push(
          `crates/${crate.crateDir}: ${crate.packageName} depends on concrete repository crate ${dep}; only a composition root may name a repository implementation`,
        );
      }
    }
  }
  assert.deepEqual(violations, []);

  // Positive control: the rule must have something to allow, or it would pass vacuously after a
  // refactor moved the construction somewhere unexpected.
  const roots = crates.filter((crate) => crate.normal.some((dep) => REPOSITORY_DEPENDENCY.test(dep)));
  assert.deepEqual(
    roots.map((crate) => crate.packageName),
    ['sdkwork-merchandise-service-host'],
    'the repository implementation must be constructed by the service host and nowhere else',
  );
});

test('the HTTP adapter consumes the port and never names a repository', () => {
  const adapter = crateByPackage.get('sdkwork-merchandise-web-support');
  assert.ok(adapter, 'the web-support crate must exist');
  assert.deepEqual(
    adapter.normal.filter((dep) => REPOSITORY_DEPENDENCY.test(dep)),
    [],
    'the HTTP adapter must not depend on a concrete repository crate',
  );
  assert.ok(
    /Arc<dyn CatalogRepositoryPort>/.test(adapter.source),
    'the adapter must hold the service-owned port, not a concrete store',
  );
  const leaks = [];
  for (const match of adapter.source.matchAll(/\b(PostgresCommerceCatalogStore|PgPool|as_postgres)\b/g)) {
    leaks.push(match[1]);
  }
  assert.deepEqual(leaks, [], 'the adapter must not name a driver type, a pool, or a concrete store');

  // The route crate composes, but it does not construct: it asks the host for the port.
  const routes = crateByPackage.get('sdkwork-routes-merchandise-backend-api');
  assert.ok(routes, 'the route crate must exist');
  assert.deepEqual(
    routes.normal.filter((dep) => REPOSITORY_DEPENDENCY.test(dep)),
    [],
    'a route crate must not depend on a concrete repository crate',
  );
  assert.ok(
    /build_backend_catalog_router\(host\.catalog_repository\(\)\)/.test(routes.source),
    'the route crate must mount the routes over the port the composition root hands it',
  );
});

test('the committed composition resolution advertises exactly the ports the specs declare', () => {
  const resolution = JSON.parse(readText('generated/composition.resolved.json'));
  const advertised = new Set();
  const walk = (node) => {
    if (Array.isArray(node)) {
      for (const entry of node) walk(entry);
      return;
    }
    if (!node || typeof node !== 'object') return;
    if (typeof node.target === 'string') advertised.add(node.target);
    for (const value of Object.values(node)) walk(value);
  };
  walk(resolution);
  assert.deepEqual(
    [...advertised].sort(),
    boundPorts.map((port) => port.target).sort(),
    'generated/composition.resolved.json is a committed artifact; re-run resolve-composition.mjs --write after editing a port',
  );
});
