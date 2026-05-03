#!/usr/bin/env node
// provekit-lsp-ts CLI launcher.
//
// Mirrors bin/provekit-lift.cjs: tsx-driven, no precompiled dist/.
// The underlying daemon is at
//   implementations/typescript/src/lsp/daemon.ts

const { spawn } = require("child_process");
const path = require("path");

const tsxCli = require.resolve("tsx/cli");
const target = path.resolve(
  __dirname,
  "..",
  "implementations",
  "typescript",
  "src",
  "lsp",
  "daemon-entry.ts",
);

const child = spawn(
  process.execPath,
  [tsxCli, target, ...process.argv.slice(2)],
  { stdio: "inherit" },
);

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});
