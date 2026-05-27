/**
 * Demo: lift native TypeScript source and emit the derived IR contracts.
 *
 * Premise: the user's ordinary source file is the contract surface. The
 * TypeScript source lifter reads function bodies and emits function-contract
 * mementos. No hand-authored contract file participates.
 *
 * Run: npx tsx scripts/cross-language-demo/runtime-eval/run-and-emit-ir.ts
 */

import { writeFileSync, mkdirSync, existsSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { liftTypeScriptSourceText } from "../../../implementations/typescript/src/lift/typescript-source/index.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = join(__dirname, "..", "..", "output", "runtime-eval");
const SOURCE_PATH = "scripts/cross-language-demo/runtime-eval/native-source.ts";
const SOURCE_FILE = join(__dirname, "native-source.ts");
if (!existsSync(OUTPUT_DIR)) mkdirSync(OUTPUT_DIR, { recursive: true });

console.log("Runtime-eval native-source lifting demo");
console.log("=".repeat(70));
console.log();
console.log("The user's native source file:");
console.log(`  ${SOURCE_PATH}`);
console.log();
console.log("Lifting strategy: run the TypeScript source lifter on the file.");
console.log("The native function bodies become function-contract mementos.");
console.log();

async function main(): Promise<void> {
  const sourceText = readFileSync(SOURCE_FILE, "utf8");
  const result = liftTypeScriptSourceText(sourceText, SOURCE_PATH);
  const declarations = result.declarations;

  console.log(`Lifted ${declarations.length} function-contract declaration(s):`);
  console.log();

  for (const decl of declarations) {
    console.log(`  ${decl.kind.padEnd(17)} ${decl.fnName}`);
    console.log(`                    formals: ${decl.formals.join(", ") || "(none)"}`);
    console.log(`                    effects: ${decl.effects.map((e) => e.kind).join(", ") || "(none)"}`);
  }

  console.log();

  if (result.refusals.length > 0) {
    console.log(`Refusals: ${result.refusals.length}`);
    for (const refusal of result.refusals) {
      console.log(`  ${refusal.function ?? "(unknown)"}: ${refusal.reason}`);
    }
    console.log();
  }

  // Dump the full IR to disk for inspection.
  const irOutput = JSON.stringify(result, null, 2);
  writeFileSync(join(OUTPUT_DIR, "lifted-declarations.json"), irOutput);

  console.log(`IR dump written to:`);
  console.log(`  ${join(OUTPUT_DIR, "lifted-declarations.json")}`);
  console.log(`  ${irOutput.length} bytes`);
  console.log();

  console.log("=".repeat(70));
  console.log("That's the native-source path: source file -> lifter -> IR.");
  console.log();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
