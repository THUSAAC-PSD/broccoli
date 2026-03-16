import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import type { Plugin, ResolvedConfig } from 'vite';

/**
 * Vite plugin that enables dynamically loaded plugins (from the backend) to
 * import shared dependencies like `react` via bare specifiers.
 *
 * It does two things:
 *   1. Exposes a virtual module (`virtual:shared-deps-map`) that returns
 *      a mapping of bare specifiers → browser-resolvable URLs.
 *   2. Emits thin re-export "shim" modules for each shared dep. In dev via
 *      middleware (re-exporting from Vite's pre-bundled deps), in production via
 *      emitted chunks.
 *
 * The consumer (root.tsx) injects the map as a `<script type="importmap">`
 * in `<head>`, which the browser uses to resolve bare imports in any
 * dynamically loaded module (including cross-origin plugin bundles).
 */

export const SDK_SHARED_DEPS = [
  '@broccoli/web-sdk',
  '@broccoli/web-sdk/api',
  '@broccoli/web-sdk/auth',
  '@broccoli/web-sdk/contest',
  '@broccoli/web-sdk/hooks',
  '@broccoli/web-sdk/i18n',
  '@broccoli/web-sdk/plugin',
  '@broccoli/web-sdk/problem',
  '@broccoli/web-sdk/sidebar',
  '@broccoli/web-sdk/slot',
  '@broccoli/web-sdk/submission',
  '@broccoli/web-sdk/theme',
  '@broccoli/web-sdk/ui',
  '@broccoli/web-sdk/utils',
] as const;

export const SHARED_DEPS = [
  'react',
  'react/jsx-runtime',
  'react/jsx-dev-runtime',
  'react-dom',
  'react-dom/client',
  'react-router',
  '@tanstack/react-query',
  ...SDK_SHARED_DEPS,
  'lucide-react',
  '@monaco-editor/react',
] as const;

const DEFAULT_EXPORT_DEPS = new Set<(typeof SHARED_DEPS)[number]>([
  'react',
  'react/jsx-runtime',
  'react/jsx-dev-runtime',
  'react-dom',
  'react-dom/client',
  '@monaco-editor/react',
]);

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
 * These re-export from Vite's pre-bundled deps so dynamically loaded plugins
 * (which bypass Vite's import rewriting) can resolve bare specifiers.
 */
const DEV_SHIM_PREFIX = '/@shared-deps/';

/** Reverse lookup from flattened ids back to original dep names. */
const FLAT_TO_DEP = new Map(SHARED_DEPS.map((dep) => [flattenId(dep), dep]));

const IDENT_RE = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/;
const SDK_DIST_ROOT = fileURLToPath(
  new URL('../../sdk/dist/', import.meta.url),
);

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

/** Mirrors Vite's dep pre-bundling filename convention: `/` → `_` */
function flattenId(dep: string): string {
  return dep.replace(/\//g, '_');
}

function isSdkDep(dep: string): dep is (typeof SDK_SHARED_DEPS)[number] {
  return dep === '@broccoli/web-sdk' || dep.startsWith('@broccoli/web-sdk/');
}

function resolveSdkEntry(dep: (typeof SDK_SHARED_DEPS)[number]): string {
  try {
    return fileURLToPath(import.meta.resolve(dep));
  } catch {
    if (dep === '@broccoli/web-sdk') {
      return resolve(SDK_DIST_ROOT, 'index.js');
    }

    return resolve(
      SDK_DIST_ROOT,
      dep.slice('@broccoli/web-sdk/'.length),
      'index.js',
    );
  }
}

function toFsModuleUrl(absPath: string): string {
  return `/@fs/${absPath.replace(/\\/g, '/').replace(/^\/+/, '')}`;
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
 * Generate a dev-mode ESM shim that re-exports a pre-bundled dependency.
 *
 * Vite pre-bundles CJS deps (React) as `export default module.exports` with
 * no named exports. ESM deps like `@tanstack/react-query` keep their real named
 * exports but have no default. We handle both via namespace import:
 *
 * ```
 *   import * as __ns gives `{ default: cjsExports }` for CJS
 *   and `{ useQuery, ... }` for ESM.
 * ```
 *
 * Then `__ns.default ?? __ns` gives the right object to extract names from.
 *
 * The `?v={browserHash}` query is critical, because without it, the browser would
 * load a separate module instance (URL mismatch leads to duplicate React and broken
 * hooks).
 */
function generateDevShim(dep: string, browserHash: string): string {
  const named = getNamedExports(dep);
  const hashSuffix = browserHash ? `?v=${browserHash}` : '';
  const importSource = isSdkDep(dep)
    ? toFsModuleUrl(resolveSdkEntry(dep))
    : `/node_modules/.vite/deps/${flattenId(dep)}.js${hashSuffix}`;

  const lines = [`import * as __ns from '${importSource}';`];

  if (DEFAULT_EXPORT_DEPS.has(dep as (typeof SHARED_DEPS)[number])) {
    lines.push(`const __mod = __ns.default;`);
    lines.push(`export default __mod;`);
    for (const name of named) {
      lines.push(`export const ${name} = __mod.${name};`);
    }
  } else {
    lines.push(`export * from '${importSource}';`);
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
      server.middlewares.use((req, res, next) => {
        if (!req.url?.startsWith(DEV_SHIM_PREFIX)) return next();

        const browserHash = readBrowserHash(config.cacheDir);
        const reqPath = req.url.split('?', 1)[0];

        const depFlat = reqPath
          .slice(DEV_SHIM_PREFIX.length)
          .replace(/\.js$/, '');
        const dep = FLAT_TO_DEP.get(depFlat);

        if (!dep) {
          res.statusCode = 404;
          res.end('Not a shared dep');
          return;
        }

        const code = generateDevShim(dep, browserHash);
        res.setHeader('Content-Type', 'application/javascript');
        res.setHeader('Cache-Control', 'no-cache');
        res.end(code);
      });

      server.middlewares.use((_, res, next) => {
        const browserHash = readBrowserHash(config.cacheDir);
        const hashSuffix = browserHash ? `?v=${browserHash}` : '';
        const imports: Record<string, string> = {};
        for (const dep of SHARED_DEPS) {
          imports[dep] = `${DEV_SHIM_PREFIX}${flattenId(dep)}.js${hashSuffix}`;
        }
        const importMapTag = `<script type="importmap">${JSON.stringify({ imports })}</script>`;

        let injected = false;
        const _end = res.end.bind(res);
        const _write = res.write.bind(res);

        function tryInject(chunk: unknown, enc?: string): unknown {
          if (injected || chunk == null) return chunk;
          const ct = res.getHeader('content-type');
          if (!ct || !String(ct).includes('text/html')) return chunk;

          const encoding = (enc || 'utf-8') as BufferEncoding;
          const str =
            typeof chunk === 'string'
              ? chunk
              : Buffer.isBuffer(chunk)
                ? chunk.toString(encoding)
                : null;
          if (str === null) return chunk;

          const match = /<head(\s[^>]*)?>|<head>/.exec(str);
          if (!match) return chunk;

          injected = true;
          const at = match.index + match[0].length;
          const patched = str.slice(0, at) + importMapTag + str.slice(at);
          return typeof chunk === 'string'
            ? patched
            : Buffer.from(patched, encoding);
        }

        res.write = function (
          chunk: unknown,
          encOrCb?: BufferEncoding | ((err?: Error | null) => void),
          cb?: (err?: Error | null) => void,
        ): boolean {
          const enc = typeof encOrCb === 'string' ? encOrCb : undefined;
          const patched = tryInject(chunk, enc);
          if (typeof encOrCb === 'function')
            return _write(patched as string, encOrCb);
          return _write(patched as string, encOrCb as BufferEncoding, cb);
        } as typeof res.write;

        res.end = function (
          chunk?: unknown,
          encOrCb?: BufferEncoding | ((err?: Error | null) => void),
          cb?: (err?: Error | null) => void,
        ) {
          const enc = typeof encOrCb === 'string' ? encOrCb : undefined;
          const patched = tryInject(chunk, enc);
          if (typeof encOrCb === 'function')
            return _end(patched as string, encOrCb);
          return _end(patched as string, encOrCb as BufferEncoding, cb);
        } as typeof res.end;

        next();
      });
    },

    resolveId(id) {
      if (isDev && isSdkDep(id)) {
        return `${DEV_SHIM_PREFIX}${flattenId(id)}.js`;
      }
      if (id === VIRTUAL_MAP_ID) return RESOLVED_MAP_ID;
      if (id.startsWith(SHIM_PREFIX)) return '\0' + id;
      return null;
    },

    load(id) {
      // Virtual import map module
      if (id === RESOLVED_MAP_ID) {
        if (isDev) {
          return `export default {};`;
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
        if (!DEFAULT_EXPORT_DEPS.has(dep as (typeof SHARED_DEPS)[number])) {
          return `export * from '${dep}';`;
        }

        const named = getNamedExports(dep);
        const lines = [
          `import * as __ns from '${dep}';`,
          `const __mod = __ns.default;`,
          `export default __mod;`,
        ];
        for (const name of named) {
          lines.push(`export const ${name} = __mod.${name};`);
        }
        return lines.join('\n');
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
