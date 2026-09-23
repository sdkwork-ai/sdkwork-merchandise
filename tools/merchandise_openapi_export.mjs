#!/usr/bin/env node

import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const checkMode = process.argv.includes('--check');
const contractPath = path.join(
  root,
  'apis',
  'backend-api',
  'merchandise',
  'shop-backend-api.merchandise.openapi.json',
);
// The generated document is LF, while a Windows working tree checks the same
// file out as CRLF (this repository carries no `.gitattributes`). Normalising
// before comparing keeps this gate a content check instead of a line-ending
// check, which would otherwise fail unconditionally on Windows.
const current = readFileSync(contractPath, 'utf8').replace(/\r\n/g, '\n');
const document = JSON.parse(current);
const HTTP_METHODS = new Set(['get', 'post', 'put', 'patch', 'delete']);
let operationCount = 0;

if (document.info?.['x-sdkwork-api-authority'] !== 'sdkwork-shop-backend-api') {
  throw new Error('merchandise routes must aggregate into sdkwork-shop-backend-api');
}
if (document.info?.['x-sdkwork-owner'] !== 'sdkwork-shop') {
  throw new Error('merchandise backend route contract owner must be sdkwork-shop');
}

for (const [operationPath, pathItem] of Object.entries(document.paths ?? {})) {
  for (const [method, operation] of Object.entries(pathItem ?? {})) {
    if (!HTTP_METHODS.has(method)) continue;
    operationCount += 1;
    operation['x-sdkwork-owner'] = 'sdkwork-shop';
    operation['x-sdkwork-api-authority'] = 'sdkwork-shop-backend-api';
    operation['x-sdkwork-api-surface'] = 'backend-api';
    operation['x-sdkwork-request-context'] = 'WebRequestContext';
    operation['x-sdkwork-auth-mode'] = 'dual-token';
    operation['x-sdkwork-permission'] = method === 'get'
      ? 'commerce.catalog.read'
      : 'commerce.catalog.manage';
    operation['x-sdkwork-source-route-crate'] = 'sdkwork-routes-merchandise-backend-api';
    operation['x-sdkwork-source'] =
      `sdkwork-routes-merchandise-backend-api:${operationPath}`;
  }
}

if (operationCount === 0) throw new Error('merchandise backend route contract is empty');
const expected = `${JSON.stringify(document, null, 2)}\n`;
if (checkMode && expected !== current) {
  throw new Error('merchandise backend route contract is not synchronized');
}
if (!checkMode && expected !== current) writeFileSync(contractPath, expected, 'utf8');
process.stdout.write(
  `[merchandise_openapi_export] ${checkMode ? 'check passed' : 'materialized'} (${operationCount} operations)\n`,
);
