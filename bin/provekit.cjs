#!/usr/bin/env node
// provekit CLI launcher.
//
// v1 ships as a tsx-driven binary rather than precompiled dist/. This avoids
// the ESM/CJS mismatch the project carries today: ~14 source files use
// `import.meta.url` for ESM-style __filename/__dirname while the package is
// declared `"type": "commonjs"` with `module: commonjs` in tsconfig. Switching
// the whole project to ESM is a separate, larger task; this wrapper unblocks
// `npm install -g provekit` distribution channel without that conversion.
//
// tsx is a dependency (not devDependency) so it ships with the global install.
// next/vite/vitest ship similarly under the hood: this is a known-good shape.

const { spawn } = require("child_process");
const path = require("path");

const tsxCli = require.resolve("tsx/cli");
const target = path.resolve(__dirname, "..", "implementations", "typescript", "src", "cli.ts");

const child = spawn(
  process.execPath,
  [tsxCli, target, ...process.argv.slice(2)],
  { stdio: "inherit" }
);

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});
