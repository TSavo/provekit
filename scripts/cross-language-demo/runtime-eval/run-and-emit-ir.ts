/**
 * Demo: run an invariant file and emit the IR.
 *
 * Premise: the user's invariant file imports symbolic primitives.
 * Running the file produces the IR via the collector.
 *
 * No tsc Compiler API. No AST walking. Just import the file inside
 * a collector context; the IR comes out.
 *
 * Run: npx tsx scripts/cross-language-demo/runtime-eval/run-and-emit-ir.ts
 */

import { beginCollecting } from "../../../src/ir/symbolic/index.js";
import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = join(__dirname, "..", "..", "output", "runtime-eval");
if (!existsSync(OUTPUT_DIR)) mkdirSync(OUTPUT_DIR, { recursive: true });

console.log("Runtime-eval lifting demo");
console.log("=".repeat(70));
console.log();
console.log("The user's invariant file:");
console.log("  scripts/cross-language-demo/runtime-eval/parseInt.invariant.ts");
console.log();
console.log("Lifting strategy: import the file inside beginCollecting().");
console.log("The describe/must/bridge calls register declarations with the");
console.log("active collector. No AST walking. No tsc Compiler API.");
console.log();

async function main(): Promise<void> {
const finish = beginCollecting();

// THIS is the entire lifter. Import the file. The file's top-level
// code runs; describe()/must() execute; declarations land in the
// collector. Done.
await import("./parseInt.invariant.js");

const declarations = finish();

console.log(`Lifted ${declarations.length} declarations:`);
console.log();

for (const decl of declarations) {
  console.log(`  ${decl.kind.padEnd(8)} ${decl.name}`);
  if (decl.kind === "property") {
    console.log(`           formula.kind: ${decl.formula.kind}`);
  } else if (decl.kind === "bridge") {
    console.log(`           ${decl.sourceSymbol} → ${decl.targetLayer}`);
  }
}

console.log();

// Dump the full IR to disk for inspection.
const irOutput = JSON.stringify(declarations, null, 2);
writeFileSync(join(OUTPUT_DIR, "lifted-declarations.json"), irOutput);

console.log(`IR dump written to:`);
console.log(`  ${join(OUTPUT_DIR, "lifted-declarations.json")}`);
console.log(`  ${irOutput.length} bytes`);
console.log();

console.log("=".repeat(70));
console.log("That's the entire lifter. Importing the file IS lifting.");
console.log("The IR data structures fall out as a natural consequence");
console.log("of running the kit's symbolic primitives.");
console.log();
console.log("No tsc dependency for this path.");
console.log("No AST walker.");
console.log("No visitor pattern.");
console.log("Just function calls returning IR data.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
