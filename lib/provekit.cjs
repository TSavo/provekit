// provekit library entry — CJS shim.
//
// v1 ships the library surface as a tsx-driven runtime require rather than
// a precompiled dist/. This mirrors the channel-1 strategy used by
// bin/provekit.cjs: route around the project's outstanding ESM/CJS mismatch
// (~14 source files use `import.meta.url` while the package is declared
// `"type": "commonjs"`) by registering tsx's CJS hook and requiring the
// TypeScript entry directly.
//
// tsx is a runtime dependency (not devDependency) so it ships with
// `npm install provekit`. esbuild and get-tsconfig come along as tsx's
// own deps. next/vite/vitest ship similarly under the hood — this is a
// known-good shape.
//
// The full ESM conversion is a separate, larger task tracked outside this
// shim. When that lands, this file becomes a one-liner re-export of the
// compiled dist/index.js.

require("tsx/cjs/api").register();

module.exports = require("../src/index.ts");
