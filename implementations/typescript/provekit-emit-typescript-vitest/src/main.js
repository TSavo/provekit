#!/usr/bin/env node
"use strict";

const { runRpc } = require("./rpc");

if (process.argv.includes("--rpc")) {
  runRpc();
} else {
  process.stderr.write("usage: provekit-emit-typescript-vitest --rpc\n");
  process.exit(2);
}
