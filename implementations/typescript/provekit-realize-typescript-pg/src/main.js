#!/usr/bin/env node
const { runRpc } = require("./rpc");

function main(argv = process.argv.slice(2)) {
  if (argv.includes("--rpc")) {
    runRpc();
    return;
  }
  process.stdout.write("usage: provekit-realize-typescript-pg --rpc\n");
}

if (require.main === module) {
  main();
}

module.exports = { main };
