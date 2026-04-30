// Cross-language equivalence runner — TypeScript path.
//
// Usage: tsx ts-runner.ts <fixture-name>
// Emits: compact JSON of the Declaration[] for the named fixture.
//
// All kits (TS, Rust, Go, C++) MUST produce byte-identical compact JSON
// for the same fixture name. The harness diffs and hashes the outputs.

import {
  forAll,
  gt,
  num,
  Int,
  beginCollecting,
  property,
  _resetCollector,
} from "../../src/ir/symbolic/index.js";

const fixture = process.argv[2];
if (!fixture) {
  console.error("usage: ts-runner.ts <fixture-name>");
  process.exit(2);
}

_resetCollector();
const finish = beginCollecting();

switch (fixture) {
  case "forall_int_gt_zero":
    property("forall_int_gt_zero", forAll(Int, (x) => gt(x, num(0))));
    break;
  default:
    console.error(`unknown fixture: ${fixture}`);
    process.exit(2);
}

const decls = finish();
process.stdout.write(JSON.stringify(decls));
