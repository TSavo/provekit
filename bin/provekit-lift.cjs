#!/usr/bin/env node
// provekit-lift CLI launcher.
//
// Mirrors bin/provekit.cjs: tsx-driven, no precompiled dist/. The
// underlying CLI is at implementations/typescript/src/lift/cli.ts.

const { spawn } = require("child_process");
const path = require("path");

const tsxCli = require.resolve("tsx/cli");
const target = path.resolve(
  __dirname,
  "..",
  "implementations",
  "typescript",
  "src",
  "lift",
  "bin",
  "main.ts",
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
