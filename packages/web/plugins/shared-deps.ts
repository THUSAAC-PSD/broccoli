import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

import type { Plugin, ResolvedConfig } from 'vite';

/**
 * Vite plugin that enables dynamically loaded plugins (from the backend) to
 * import shared dependencies like `react` via bare specifiers.
 *
 * It does two things:
 *   1. Exposes a virtual module (`virtual:shared-deps-map`) that returns
 *      a mapping of bare specifiers → browser-resolvable URLs.
 *   2. Emits thin re-export "shim" modules for each shared dep — in dev via
 *      middleware (extracting named exports from the CJS default), in production
 *      via emitted chunks.
 *
 * The consumer (root.tsx) injects the map as a `<script type="importmap">`
 * in `<head>`, which the browser uses to resolve bare imports in any
 * dynamically loaded module (including cross-origin plugin bundles).
 */

const SHARED_DEPS = [
  'react',
  'react/jsx-runtime',
  'react/jsx-dev-runtime',
  'react-dom',
  'react-dom/client',
] as const;

const VIRTUAL_MAP_ID = 'virtual:shared-deps-map';
const RESOLVED_MAP_ID = '\0' + VIRTUAL_MAP_ID;

const SHIM_PREFIX = 'shared-dep:';
const RESOLVED_SHIM_PREFIX = '\0' + SHIM_PREFIX;

/**
 * Sentinel string embedded in the client build's virtual module.
 * Replaced with the real import map in `generateBundle`, after chunk
 * filenames (with content hashes) are finalized.
 *
 * We match any quote style (double, single, backtick) because Rolldown
 * may output template literals instead of regular strings.
 */
const PLACEHOLDER_RE = /(["`'])__SHARED_DEPS_MAP__\1/;

const MANIFEST_FILENAME = 'shared-deps-map.json';

/**
 * URL prefix for dev-mode shim modules served via middleware.
 * These extract named exports from Vite's pre-bundled CJS deps (which only
 * have `export default`). Vite normally handles this by rewriting import
 * statements in consuming code, but dynamically loaded plugins bypass that.
 */
const DEV_SHIM_PREFIX = '/@shared-deps/';

const SHARED_DEPS_SET: ReadonlySet<string> = new Set(SHARED_DEPS);

const IDENT_RE = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/;

/** Mirrors Vite's dep pre-bundling filename convention: `/` → `_` */
function flattenId(dep: string): string {
  return dep.replace(/\//g, '_');
}

/**
 * Discover the named exports of a CJS dependency using Node's native
 * `require()`. This is reliable because: (1) the middleware runs in Node.js
 * where CJS works natively, and (2) `createRequire` avoids rolldown/Vite
 * intercepting the resolution (unlike `import()` or `ssrLoadModule()`).
 */
const nodeRequire = createRequire(import.meta.url);

function getNamedExports(dep: string): string[] {
  try {
    const mod = nodeRequire(dep);
    if (typeof mod !== 'object' || mod === null) return [];
    return Object.keys(mod).filter(
      (k) => k !== 'default' && k !== '__esModule' && IDENT_RE.test(k),
    );
  } catch {
    return [];
  }
}

/**
 * Read the browserHash from Vite's dep optimizer metadata.
 * This hash must be appended as `?v={hash}` so the browser resolves to the
 * exact same module instance the host app uses (module cache keys on full URL).
 */
function readBrowserHash(cacheDir: string): string {
  try {
    const metaPath = resolve(cacheDir, 'deps', '_metadata.json');
    const meta = JSON.parse(readFileSync(metaPath, 'utf-8'));
    return meta.browserHash || '';
  } catch {
    return '';
  }
}

/**
 * Generate a dev-mode ESM shim that imports the pre-bundled dep's default
 * export (which is the CJS module.exports object) and re-exports each
 * property as a named export. The `?v={browserHash}` query is critical:
 * without it, the browser would load a separate module instance from the
 * one the host app uses (URL mismatch -> duplicate React -> broken hooks).
 */
function generateDevShim(dep: string, browserHash: string): string {
  const named = getNamedExports(dep);
  const hashSuffix = browserHash ? `?v=${browserHash}` : '';
  const prebundledPath = `/node_modules/.vite/deps/${flattenId(dep)}.js${hashSuffix}`;

  const lines = [`import __mod from '${prebundledPath}';`];
  lines.push('export default __mod;');
  for (const name of named) {
    lines.push(`export const ${name} = __mod.${name};`);
  }
  return lines.join('\n');
}

export function sharedDepsPlugin(): Plugin {
  let isDev = false;
  let config: ResolvedConfig;

  return {
    name: 'broccoli:shared-deps',

    config(_, { command }) {
      isDev = command === 'serve';
    },

    configResolved(resolved) {
      config = resolved;
    },

    configureServer(server) {
      let browserHash = '';

      server.middlewares.use((req, res, next) => {
        if (!req.url?.startsWith(DEV_SHIM_PREFIX)) return next();

        // Lazily read the hash (deps may not be optimized at server start).
        if (!browserHash) {
          browserHash = readBrowserHash(config.cacheDir);
        }

        const depFlat = req.url
          .slice(DEV_SHIM_PREFIX.length)
          .replace(/\.js$/, '');
        const dep = depFlat.replaceAll('_', '/');

        if (!SHARED_DEPS_SET.has(dep)) {
          res.statusCode = 404;
          res.end('Not a shared dep');
          return;
        }

        const code = generateDevShim(dep, browserHash);
        res.setHeader('Content-Type', 'application/javascript');
        res.setHeader('Cache-Control', 'no-cache');
        res.end(code);
      });
    },

    resolveId(id) {
      if (id === VIRTUAL_MAP_ID) return RESOLVED_MAP_ID;
      if (id.startsWith(SHIM_PREFIX)) return '\0' + id;
    },

    load(id) {
      // Virtual import map module
      if (id === RESOLVED_MAP_ID) {
        if (isDev) {
          // Point to dev shim modules served by configureServer middleware.
          const imports: Record<string, string> = {};
          for (const dep of SHARED_DEPS) {
            imports[dep] = `${DEV_SHIM_PREFIX}${flattenId(dep)}.js`;
          }
          return `export default ${JSON.stringify(imports)};`;
        }

        // For production:
        // embed a placeholder that generateBundle will replace
        // with the actual hashed chunk URLs.
        return [
          `var _m = "__SHARED_DEPS_MAP__";`,
          `export default typeof _m === "string" ? {} : _m;`,
        ].join('\n');
      }

      if (id.startsWith(RESOLVED_SHIM_PREFIX)) {
        const dep = id.slice(RESOLVED_SHIM_PREFIX.length);
        return `export { default } from '${dep}';\nexport * from '${dep}';\n`;
      }
    },

    buildStart() {
      if (isDev) return;

      for (const dep of SHARED_DEPS) {
        this.emitFile({
          type: 'chunk',
          id: `${SHIM_PREFIX}${dep}`,
          name: `shared-${flattenId(dep)}`,
        });
      }
    },

    generateBundle(_, bundle) {
      if (isDev) return;

      const base = config.base || '/';
      const imports: Record<string, string> = {};

      for (const [filename, chunk] of Object.entries(bundle)) {
        if (chunk.type !== 'chunk' || !chunk.facadeModuleId) continue;
        if (!chunk.facadeModuleId.startsWith(RESOLVED_SHIM_PREFIX)) continue;

        const dep = chunk.facadeModuleId.slice(RESOLVED_SHIM_PREFIX.length);
        imports[dep] = `${base}${filename}`;
      }

      const replacement = JSON.stringify(imports);
      for (const chunk of Object.values(bundle)) {
        if (chunk.type === 'chunk' && PLACEHOLDER_RE.test(chunk.code)) {
          chunk.code = chunk.code.replace(PLACEHOLDER_RE, () => replacement);
        }
      }

      // Also emit as a standalone JSON asset (useful for debugging / tooling)
      this.emitFile({
        type: 'asset',
        fileName: MANIFEST_FILENAME,
        source: JSON.stringify(imports, null, 2),
      });
    },
  };
}
