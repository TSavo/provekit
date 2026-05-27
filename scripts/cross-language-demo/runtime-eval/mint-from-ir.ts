/**
 * Mint from IR: the end-to-end pipeline.
 *
 *   native-source.ts (ordinary TypeScript)
 *      ↓ TypeScript source lifter
 *   FunctionContractMemento[] (in-memory IR)
 *      ↓ hash each function-contract memento
 *   contract CID per declaration
 *      ↓ mintMemento
 *   Signed mementos (per declaration)
 *      ↓ mintMemento with all CIDs in inputCids
 *   Catalog root memento
 *      ↓ catalog.cid → package.json provekit.proofHash
 *
 * The kit author's publishing pipeline. Run it; out comes the catalog
 * + per-declaration mementos + the proofHash to put in package.json.
 *
 * Run: npx tsx scripts/cross-language-demo/runtime-eval/mint-from-ir.ts
 */

import { writeFileSync, mkdirSync, existsSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { generateKeypair } from "../../../implementations/typescript/src/producerKeys/index.js";
import {
  mintMemento,
  VARIANT_SCHEMA_CIDS,
} from "../../../implementations/typescript/src/claimEnvelope/index.js";
import type { ClaimEnvelope } from "../../../implementations/typescript/src/claimEnvelope/types.js";
import {
  functionContractCid,
  liftTypeScriptSourceText,
} from "../../../implementations/typescript/src/lift/typescript-source/index.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = join(__dirname, "..", "..", "output", "minted-from-ir");
const PER_DECL_DIR = join(OUTPUT_DIR, "declarations");
const SOURCE_PATH = "scripts/cross-language-demo/runtime-eval/native-source.ts";
const SOURCE_FILE = join(__dirname, "native-source.ts");
if (!existsSync(PER_DECL_DIR)) mkdirSync(PER_DECL_DIR, { recursive: true });

const KIT_NAME = "@provekit/proofs/ts-types";
const KIT_VERSION = "1.0.0";
const PRODUCER_ID = "ts-kit@1.0";
const EPOCH = new Date(0).toISOString();

const KEY_SEED = Buffer.from("mint-from-ir-demo-seed-32-bytes!").subarray(0, 32);
const keypair = generateKeypair({ seed: KEY_SEED });

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function safeName(s: string): string {
  return s.replace(/\W+/g, "_");
}

async function main(): Promise<void> {
  console.log(`Minting from IR: ${KIT_NAME}@${KIT_VERSION}`);
  console.log("=".repeat(70));
  console.log();

  // -------------------------------------------------------------------------
  // Step 1: lift the native source file -> collect function contracts
  // -------------------------------------------------------------------------

  console.log("Step 1: Lift the native TypeScript source file");
  const sourceText = readFileSync(SOURCE_FILE, "utf8");
  const liftResult = liftTypeScriptSourceText(sourceText, SOURCE_PATH);
  if (liftResult.refusals.length > 0) {
    throw new Error(`native source lift refused ${liftResult.refusals.length} item(s)`);
  }
  const declarations = liftResult.declarations;
  console.log(`  ${declarations.length} function-contract declaration(s) collected`);
  console.log();

  // -------------------------------------------------------------------------
  // Step 2: per declaration, canonicalize → propertyHash → mint memento
  // -------------------------------------------------------------------------

  console.log("Step 2: Hash each function-contract memento and mint an envelope");
  const mementos: ClaimEnvelope[] = [];

  for (const decl of declarations) {
    const contractCid = functionContractCid(decl);
    const bindingHash = hash16(`${KIT_NAME}:${decl.fnName}`);

    const memento = mintMemento({
      bindingHash,
      propertyHash: contractCid,
      verdict: "holds",
      producedBy: PRODUCER_ID,
      producedAt: EPOCH,
      inputCids: [],
      evidence: {
        kind: "legacy-witness",
        schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
        body: {
          rawWitness: JSON.stringify(decl),
          legacyProducerId: PRODUCER_ID,
        },
      },
      privateKey: keypair.privateKey,
    });

    console.log(
      `  contract ${decl.fnName.padEnd(74)} contractCid: ${contractCid}`,
    );

    mementos.push(memento);
    writeFileSync(
      join(PER_DECL_DIR, `${safeName(decl.fnName)}.json`),
      JSON.stringify(memento, null, 2),
    );
  }
  console.log();

  // -------------------------------------------------------------------------
  // Step 3: compose all memento CIDs into a catalog root
  // -------------------------------------------------------------------------

  console.log("Step 3: Compose all memento CIDs into a catalog root");
  const inputCids = mementos.map((m) => m.cid).sort();

  const catalogRoot = mintMemento({
    bindingHash: hash16(`${KIT_NAME}@${KIT_VERSION}`),
    propertyHash: hash16(`catalog-root:${KIT_NAME}@${KIT_VERSION}`),
    verdict: "holds",
    producedBy: PRODUCER_ID,
    producedAt: EPOCH,
    inputCids,
    evidence: {
      kind: "legacy-witness",
      schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
      body: {
        rawWitness: JSON.stringify({
          kind: "kit-catalog-root",
          kitName: KIT_NAME,
          kitVersion: KIT_VERSION,
          memberCount: inputCids.length,
          sourceFile: SOURCE_PATH,
          construction: "lift-from-native-source",
        }),
        legacyProducerId: PRODUCER_ID,
      },
    },
    privateKey: keypair.privateKey,
  });

  writeFileSync(
    join(OUTPUT_DIR, "ts-kit-catalog.json"),
    JSON.stringify(catalogRoot, null, 2),
  );

  console.log(`  catalog root cid: ${catalogRoot.cid}`);
  console.log(`  composing ${inputCids.length} declarations`);
  console.log();

  // -------------------------------------------------------------------------
  // Step 4: emit package.json fragment with provekit.proofHash
  // -------------------------------------------------------------------------

  console.log("Step 4: Emit package.json fragment with provekit.proofHash");

  const publicKeyB64 = keypair.publicKey
    .export({ type: "spki", format: "der" })
    .toString("base64");

  writeFileSync(join(OUTPUT_DIR, "public-key.b64"), publicKeyB64 + "\n");

  const packageFragment = {
    name: KIT_NAME,
    version: KIT_VERSION,
    description: `${KIT_NAME} proofs catalog`,
    files: ["dist/", "lib/", ".provekit/"],
    provekit: {
      proofHash: catalogRoot.cid,
      catalogPath: ".provekit/ts-kit-catalog.json",
      kitVersion: PRODUCER_ID,
      publicKey: publicKeyB64,
      sourceFile: SOURCE_PATH,
      construction: "lift-from-native-source",
    },
  };

  writeFileSync(
    join(OUTPUT_DIR, "package.json.fragment"),
    JSON.stringify(packageFragment, null, 2) + "\n",
  );

  console.log(`  proofHash: ${catalogRoot.cid}`);
  console.log();

  // -------------------------------------------------------------------------
  // Summary
  // -------------------------------------------------------------------------

  console.log("=".repeat(70));
  console.log("Output structure:");
  console.log(`  ${OUTPUT_DIR}/`);
  console.log(`    ts-kit-catalog.json      (the catalog root memento)`);
  console.log(`    package.json.fragment    (provekit.proofHash field)`);
  console.log(`    public-key.b64           (consumer-side verification key)`);
  console.log(`    declarations/*.json      (${mementos.length} per-declaration mementos)`);
  console.log();
  console.log("End-to-end:");
  console.log(`  source file → lift → mint → catalog`);
  console.log(`  ${declarations.length} declarations × 1 memento each + 1 catalog root`);
  console.log(`  proofHash = ${catalogRoot.cid}`);
  console.log();
  console.log("This is the kit author's publishing pipeline. Run it on the");
  console.log("kit's native source files; out comes the catalog + the");
  console.log("proofHash to put in package.json. Consumers install the");
  console.log("package; their proofkit reads provekit.proofHash; they have");
  console.log("the kit's full memento set locally.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
