// Standalone demo: prove the prototype works end-to-end without an LSP
// transport. Reads sample-invariant.json, parses per IR formal grammar,
// computes propertyHash per canonicalization spec, prints a "hover card."
//
// No sugar dependency. Each step cites the spec section it implements.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { parseDocument } from "./parse.mjs";
import { propertyHash } from "./canonicalize.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const samplePath = join(here, "sample-invariant.json");
const json = readFileSync(samplePath, "utf8");

console.log("=== Step 1: parse per ir-formal-grammar.md ===");
const decls = parseDocument(json);
console.log(`  parsed ${decls.length} declaration(s)`);
for (const d of decls) {
  console.log(`  - ${d.kind}: name=${d.name}`);
}

console.log("\n=== Step 2: compute propertyHash per canonicalization-grammar.md ===");
for (const d of decls) {
  if (d.kind !== "property") continue;
  const cid = propertyHash(d.formula);
  console.log(`  property "${d.name}" propertyHash: ${cid}`);
  console.log(`    (spec: passes 1..6 §8 + JCS §7.3 + sha256-prefix-16 §9)`);
}

console.log("\n=== Step 3: build a hover card for the user-facing surface ===");
for (const d of decls) {
  if (d.kind !== "property") continue;
  const cid = propertyHash(d.formula);
  console.log("");
  console.log(`  ${d.name}  (property declaration)`);
  console.log(`    formula kind:   ${d.formula.kind}`);
  console.log(`    propertyHash:   ${cid}`);
  console.log(`    canonicalForm:  jcs-rfc8785`);
  console.log(`    spec source:    protocol/specs/2026-04-30-canonicalization-grammar.md §3, §7.3, §9`);
}

console.log("\n=== Demonstration complete. ===");
console.log("No imports from src/, no imports from kits/*, no sugar package.");
console.log("Everything above was implementable from the protocol spec docs alone.");
