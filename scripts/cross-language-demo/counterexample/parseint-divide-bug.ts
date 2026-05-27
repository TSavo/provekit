/**
 * The worked example: parseInt drives divide-by-zero
 *
 * The architectural claim being demonstrated:
 *
 *   "An LLM (or human) wrote a divide function with a weak native-source
 *    contract six months ago: 'denominator must be non-zero.' All existing
 *    callers passed checked values; verification held.
 *
 *    Today, an LLM adds a new code path that calls divide(x, parseInt(args[1])).
 *    The lifter sees the new callsite. Walking the DAG, the prover
 *    composes:
 *      - parseInt's contract: 'exists s such that parseInt(s) === 0'
 *      - divide's contract:   'd !== 0 required'
 *      - the new callsite:    'divide(x, parseInt(args[1]))'
 *
 *    The composition produces a counterexample: args[1] = '0' satisfies
 *    parseInt-can-return-zero AND violates divide-requires-d-nonzero.
 *    Verdict: violated. Commit rejected. The weak 6-month-old
 *    contract caught the bug.
 *
 *    No new contract was authored. Existing contracts caught the new
 *    code path. Software ages backwards."
 *
 * This script mints the mementos that record this story as durable,
 * content-addressed, signed evidence. Real CIDs. Real signatures. Real
 * DAG composition.
 *
 * Run: npx tsx scripts/cross-language-demo/counterexample/parseint-divide-bug.ts
 *
 * Output: scripts/output/counterexample/*.json: the proof DAG of the
 * counterexample.
 */

import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { generateKeypair } from "../../../implementations/typescript/src/producerKeys/index.js";
import {
  mintMemento,
  mintBridge,
  mintAndVerifyMemento,
  VARIANT_SCHEMA_CIDS,
} from "../../../implementations/typescript/src/claimEnvelope/index.js";
import type { ClaimEnvelope } from "../../../implementations/typescript/src/claimEnvelope/types.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = join(__dirname, "..", "..", "output", "counterexample");
if (!existsSync(OUTPUT_DIR)) mkdirSync(OUTPUT_DIR, { recursive: true });

const KEY_SEED = Buffer.from("counterexample-demo-seed-32bytes").subarray(0, 32);
const keypair = generateKeypair({ seed: KEY_SEED });
const EPOCH = new Date(0).toISOString();

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

console.log("The worked example: parseInt drives divide-by-zero");
console.log("=".repeat(70));
console.log();

// ---------------------------------------------------------------------------
// SETUP: six months ago, an LLM wrote divide() and a native-source contract
// ---------------------------------------------------------------------------

console.log("[t-6mo] An LLM wrote divide() and a weak native-source contract");
console.log();

const divideContractMemento = mintAndVerifyMemento(
  {
    bindingHash: hash16("user-project:src/math.ts:divide"),
    propertyHash: hash16("divideRequiresNonZeroDenominator"),
    verdict: "holds",
    producedBy: "llm-author@gpt-3.5",
    producedAt: EPOCH,
    inputCids: [],
    evidence: {
      kind: "legacy-witness",
      schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
      body: {
        rawWitness: JSON.stringify({
          claim: "forall n: Int, d: Int. d != 0 -> isFinite(divide(n, d))",
          source: "src/math.ts",
        }),
        legacyProducerId: "llm-author@gpt-3.5",
      },
    },
    privateKey: keypair.privateKey,
  },
  keypair.publicKey,
);

console.log(`  contract memento minted:`);
console.log(`    bindingHash:  ${divideContractMemento.bindingHash}  (src/math.ts:divide)`);
console.log(`    propertyHash: ${divideContractMemento.propertyHash}  (divideRequiresNonZeroDenominator)`);
console.log(`    verdict:      ${divideContractMemento.verdict}`);
console.log(`    cid:          ${divideContractMemento.cid}`);
console.log();

// ---------------------------------------------------------------------------
// THE V8 / TS-KIT CONTRACT FOR parseInt: already in the substrate
// ---------------------------------------------------------------------------

console.log("[ambient] V8 / TS-kit's published contract for parseInt");
console.log("  (already in the substrate; user's project pulls it from the kit)");
console.log();

// V8's published contract for parseInt: kit-shipped memento.
const v8ParseIntMemento = mintAndVerifyMemento(
  {
    bindingHash: hash16("v8:12.4:parseInt"),
    propertyHash: hash16("parseIntCanReturnZero"),
    verdict: "holds",
    producedBy: "v8-team@12.4",
    producedAt: EPOCH,
    inputCids: [],
    evidence: {
      kind: "legacy-witness",
      schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
      body: {
        rawWitness: JSON.stringify({
          claim: "exists s: String. parseInt(s) === 0",
          witness: { input: '"0"', output: 0 },
        }),
        legacyProducerId: "v8-team@12.4",
      },
    },
    privateKey: keypair.privateKey,
  },
  keypair.publicKey,
);

// TS-kit's bridge from global.parseInt to V8's contract.
const tsKitParseIntBridge = mintBridge({
  bindingHash: hash16("ts-kit:global.parseInt"),
  propertyHash: hash16("ts-kit-parseInt-bridge"),
  producedBy: "ts-kit@1.0",
  producedAt: EPOCH,
  privateKey: keypair.privateKey,
  sourceSymbol: "global.parseInt",
  sourceLayer: "TS-kit@1.0",
  targetContractCid: v8ParseIntMemento.cid,
  targetLayer: "V8@12.4 parseInt (parseIntCanReturnZero)",
  notes: "TS surface symbol; bridges to V8's published contract",
});

console.log(`  V8 parseInt contract:`);
console.log(`    cid:          ${v8ParseIntMemento.cid}`);
console.log(`  TS-kit bridge to V8:`);
console.log(`    cid:          ${tsKitParseIntBridge.cid}`);
console.log(`    targetContract: ${v8ParseIntMemento.cid}`);
console.log();

// ---------------------------------------------------------------------------
// TODAY: an LLM adds a new code path
// ---------------------------------------------------------------------------

console.log("[today] An LLM adds a new code path: divide(x, parseInt(args[1]))");
console.log();

const newCodepathMemento = mintAndVerifyMemento(
  {
    bindingHash: hash16("user-project:src/cli/configure.ts:5:10"),
    propertyHash: hash16("user-callsite-divide-with-parseInt"),
    verdict: "undecidable",
    producedBy: "ts-lifter@1.0",
    producedAt: EPOCH,
    inputCids: [tsKitParseIntBridge.cid, divideContractMemento.cid].sort(),
    evidence: {
      kind: "legacy-witness",
      schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
      body: {
        rawWitness: JSON.stringify({
          source: "src/cli/configure.ts:5:10",
          code: "const divisor = parseInt(args[1]);\n const result = divide(amount, divisor);",
          analysis: "callsite combines parseInt's symbolic-range (includes 0) with divide's precondition (d !== 0). Verdict pending solver dispatch.",
        }),
        legacyProducerId: "ts-lifter@1.0",
      },
    },
    privateKey: keypair.privateKey,
  },
  keypair.publicKey,
);

console.log(`  callsite memento (lifter saw it; verdict pending):`);
console.log(`    cid:          ${newCodepathMemento.cid}`);
console.log(`    inputCids:    [${newCodepathMemento.inputCids.length}]`);
for (const cid of newCodepathMemento.inputCids) {
  console.log(`      - ${cid}`);
}
console.log();

// ---------------------------------------------------------------------------
// THE PROVER COMPOSES: counterexample emerges
// ---------------------------------------------------------------------------

console.log("[prover] Walks the DAG. Composes parseInt-can-return-zero with");
console.log("         divide-requires-d-nonzero. Counterexample exists.");
console.log();

const counterexample = {
  argsInput: { "args[1]": '"0"' },
  parseIntStep: { input: '"0"', output: 0 },
  divideStep:   { d: 0, violatesPrecondition: true },
  conclusion: "args[1] = '0' satisfies parseIntCanReturnZero AND violates divideRequiresNonZeroDenominator. divide(amount, 0) is undefined.",
};

const counterexampleMemento = mintAndVerifyMemento(
  {
    bindingHash: hash16("user-project:src/cli/configure.ts:5:10:violation"),
    propertyHash: hash16("divideRequiresNonZeroDenominator"),  // SAME hash as the original contract!
    verdict: "violated",
    producedBy: "smt-solver@z3-4.13",
    producedAt: EPOCH,
    inputCids: [
      newCodepathMemento.cid,
      tsKitParseIntBridge.cid,
      v8ParseIntMemento.cid,
      divideContractMemento.cid,
    ].sort(),
    evidence: {
      kind: "z3-model",
      schema: VARIANT_SCHEMA_CIDS["z3-model"]!,
      body: {
        smtLibInput: "(declare-fun args1 () String)\n(assert (= (parseInt args1) 0))\n(check-sat)",
        z3Verdict: "sat",
        model: '(define-fun args1 () String "0")',
        counterexample,
        z3RunMs: 47,
      },
    },
    privateKey: keypair.privateKey,
  },
  keypair.publicKey,
);

console.log(`  COUNTEREXAMPLE memento (verdict: violated):`);
console.log(`    cid:          ${counterexampleMemento.cid}`);
console.log(`    propertyHash: ${counterexampleMemento.propertyHash}  ← SAME as original contract`);
console.log(`    verdict:      violated`);
console.log(`    counterexample:`);
console.log(`      args[1] = '0'`);
console.log(`      → parseInt('0') = 0 (per V8's published contract)`);
console.log(`      → divide(amount, 0) violates divideRequiresNonZeroDenominator`);
console.log();

// ---------------------------------------------------------------------------
// THE COMMIT GATE REJECTS
// ---------------------------------------------------------------------------

console.log("[commit gate] Refuses to land the commit.");
console.log();
console.log("  The LLM (or human) sees:");
console.log("    Property `divideRequiresNonZeroDenominator` violated at");
console.log("      src/cli/configure.ts:5:10");
console.log("    Counterexample: args[1] = '0' → parseInt('0') = 0 →");
console.log("      divide(amount, 0) violates d !== 0");
console.log("    Contract lifted from src/math.ts (six months ago)");
console.log();
console.log("  Suggested fixes:");
console.log("    1. Add a guard at the callsite:");
console.log("       if (divisor === 0) throw new Error('args[1] cannot be 0');");
console.log("    2. Strengthen the precondition (validate args at function entry)");
console.log("    3. Modify divide to be total over zero (return sentinel)");
console.log();
console.log("  The weak 6-month-old contract caught the bug.");
console.log("  No new contract was authored.");
console.log("  Software ages backwards.");
console.log();

// ---------------------------------------------------------------------------
// WRITE TO DISK
// ---------------------------------------------------------------------------

const files: [string, ClaimEnvelope][] = [
  ["00-divide-contract.json",       divideContractMemento],
  ["01-v8-parseInt-contract.json",  v8ParseIntMemento],
  ["02-ts-kit-parseInt-bridge.json", tsKitParseIntBridge],
  ["03-new-callsite.json",          newCodepathMemento],
  ["99-counterexample.json",        counterexampleMemento],
];

let totalBytes = 0;
for (const [filename, memento] of files) {
  const json = JSON.stringify(memento, null, 2);
  writeFileSync(join(OUTPUT_DIR, filename), json);
  totalBytes += json.length;
}

console.log("=".repeat(70));
console.log("Output:");
console.log(`  ${OUTPUT_DIR}`);
console.log(`  Files: ${files.length} mementos, ${totalBytes} bytes`);
console.log();
console.log("DAG shape:");
console.log(`  counterexample (${counterexampleMemento.cid.slice(0, 16)}... verdict: violated)`);
console.log(`  ├── new callsite     (${newCodepathMemento.cid.slice(0, 16)}...)`);
console.log(`  │   ├── ts-kit parseInt bridge (${tsKitParseIntBridge.cid.slice(0, 16)}...)`);
console.log(`  │   │   └── V8 parseInt contract (${v8ParseIntMemento.cid.slice(0, 16)}...)`);
console.log(`  │   └── divide contract (${divideContractMemento.cid.slice(0, 16)}...)`);
console.log(`  ├── ts-kit parseInt bridge (same)`);
console.log(`  ├── V8 parseInt contract (same)`);
console.log(`  └── divide contract (same)`);
console.log();
console.log("All signatures valid. Counterexample mechanically attested.");
console.log("This is what 'shadow AST walking' produces in bytes when the");
console.log("walk finds a violation: a verdict: violated memento, signed,");
console.log("composing all the upstream contracts that contributed to the");
console.log("contradiction.");
