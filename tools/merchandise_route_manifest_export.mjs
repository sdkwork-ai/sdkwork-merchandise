#!/usr/bin/env node

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const checkMode = process.argv.includes('--check');
const sourcePath = path.join(
  root,
  'apis',
  'backend-api',
  'merchandise',
  'shop-backend-api.merchandise.openapi.json',
);
const packageName = 'sdkwork-routes-merchandise-backend-api';
const authority = 'sdkwork-shop-backend-api';
const family = 'sdkwork-shop-backend-sdk';
const methodVariants = { get: 'Get', post: 'Post', put: 'Put', patch: 'Patch', delete: 'Delete' };
const document = JSON.parse(readFileSync(sourcePath, 'utf8'));
const routes = [];

for (const [operationPath, pathItem] of Object.entries(document.paths ?? {})) {
  for (const [method, variant] of Object.entries(methodVariants)) {
    const operation = pathItem?.[method];
    if (!operation) continue;
    if (operation['x-sdkwork-owner'] !== 'sdkwork-shop') {
      throw new Error(`${method.toUpperCase()} ${operationPath} owner mismatch`);
    }
    if (operation['x-sdkwork-api-authority'] !== authority) {
      throw new Error(`${method.toUpperCase()} ${operationPath} authority mismatch`);
    }
    if (operation['x-sdkwork-source-route-crate'] !== packageName) {
      throw new Error(`${method.toUpperCase()} ${operationPath} route crate mismatch`);
    }
    if (!String(operation['x-sdkwork-permission'] ?? '').trim()) {
      throw new Error(`${method.toUpperCase()} ${operationPath} permission missing`);
    }
    routes.push({ method, variant, operationPath, operation });
  }
}

const manifest = {
  schemaVersion: 1,
  kind: 'sdkwork.route.manifest',
  packageName,
  surface: 'backend-api',
  owner: 'sdkwork-shop',
  domain: 'commerce',
  capability: 'merchandise',
  apiAuthority: authority,
  sdkFamily: family,
  prefix: '/backend/v3/api',
  source: {
    crateRoot: `crates/${packageName}`,
    crateImport: 'sdkwork_routes_merchandise_backend_api',
  },
  routes: routes.map(({ method, operationPath, operation }) => ({
    method: method.toUpperCase(),
    path: operationPath,
    operationId: operation.operationId,
    tags: operation.tags ?? ['merchandise'],
    auth: {
      mode: operation['x-sdkwork-auth-mode'] ?? 'dual-token',
      required: true,
      permission: operation['x-sdkwork-permission'],
    },
    ownership: { owner: 'sdkwork-shop', apiAuthority: authority },
    source: { file: operation['x-sdkwork-source'] },
    requestContext: 'WebRequestContext',
    apiSurface: 'backend-api',
  })),
};

const rustEntries = routes.map(({ variant, operationPath, operation }) => [
  '    HttpRoute::dual_token(',
  `        HttpMethod::${variant},`,
  `        ${JSON.stringify(operationPath)},`,
  '        "merchandise",',
  `        ${JSON.stringify(operation.operationId)},`,
  '    ),',
].join('\n'));
const rust = [
  'use sdkwork_web_core::{HttpMethod, HttpRoute, HttpRouteManifest};',
  '',
  'const HTTP_ROUTES: &[HttpRoute] = &[',
  ...rustEntries,
  '];',
  '',
  'pub fn backend_route_manifest() -> HttpRouteManifest {',
  '    HttpRouteManifest::new(HTTP_ROUTES)',
  '}',
  '',
].join('\n');

function synchronize(relativePath, content) {
  const targetPath = path.join(root, relativePath);
  // Generated content is always LF; a Windows working tree checks the same
  // files out as CRLF (this repository carries no `.gitattributes`), so
  // normalise before comparing to keep this a content check rather than a
  // line-ending check.
  const current = existsSync(targetPath)
    ? readFileSync(targetPath, 'utf8').replace(/\r\n/g, '\n')
    : '';
  if (checkMode && current !== content) throw new Error(`${relativePath} is not synchronized`);
  if (!checkMode && current !== content) {
    mkdirSync(path.dirname(targetPath), { recursive: true });
    writeFileSync(targetPath, content, 'utf8');
  }
}

synchronize(
  `sdks/_route-manifests/backend-api/${packageName}.route-manifest.json`,
  `${JSON.stringify(manifest, null, 2)}\n`,
);
synchronize(
  'crates/sdkwork-routes-merchandise-backend-api/src/http_route_manifest.rs',
  rust,
);
process.stdout.write(
  `[merchandise_route_manifest_export] ${checkMode ? 'check passed' : 'materialized'} (${routes.length} routes)\n`,
);
