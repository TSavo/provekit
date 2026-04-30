/**
 * Mint from IR: the end-to-end pipeline.
 *
 *   parseInt.invariant.ts (symbolic primitives)
 *      ↓ import inside beginCollecting()
 *   Declaration[] (in-memory IR)
 *      ↓ canonicalize each property's formula
 *   propertyHash per declaration
 *      ↓ mintMemento / mintBridge
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

import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { generateKeypair } from "../../../src/producerKeys/index.js";
import { propertyHashFromFormula } from "../../../src/canonicalizer/index.js";
import {
  mintMemento,
  mintBridge,
  mintLegacyWitness,
  VARIANT_SCHEMA_CIDS,
} from "../../../src/claimEnvelope/index.js";
import type { ClaimEnvelope } from "../../../src/claimEnvelope/types.js";
import { beginCollecting } from "../../../src/ir/symbolic/index.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = join(__dirname, "..", "..", "output", "minted-from-ir");
const PER_DECL_DIR = join(OUTPUT_DIR, "declarations");
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
  // Step 1: run the invariant file → collect IR declarations
  // -------------------------------------------------------------------------

  console.log("Step 1: Run the invariant file inside beginCollecting()");
  const finish = beginCollecting();
  await import("./parseInt.invariant.js");
  const declarations = finish();
  console.log(`  ${declarations.length} declarations collected`);
  console.log();

  // -------------------------------------------------------------------------
  // Step 2: per declaration, canonicalize → propertyHash → mint memento
  // -------------------------------------------------------------------------

  console.log("Step 2: Canonicalize each property's formula and mint a memento");
  const mementos: ClaimEnvelope[] = [];

  for (const decl of declarations) {
    let memento: ClaimEnvelope;

    if (decl.kind === "property") {
      const propertyHash = propertyHashFromFormula(decl.formula);
      const bindingHash = hash16(`${KIT_NAME}:${decl.name}`);

      memento = mintMemento({
        bindingHash,
        propertyHash,
        verdict: "holds",
        producedBy: PRODUCER_ID,
        producedAt: EPOCH,
        inputCids: [],
        evidence: {
          kind: "legacy-witness",
          schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
          body: {
            rawWitness: JSON.stringify(decl.formula),
            legacyProducerId: PRODUCER_ID,
          },
        },
        privateKey: keypair.privateKey,
      });

      console.log(
        `  property ${decl.name.padEnd(60)}  propertyHash: ${propertyHash}`,
      );
    } else {
      memento = mintBridge({
        bindingHash: hash16(`${KIT_NAME}:bridge:${decl.name}`),
        propertyHash: hash16(`bridge:${decl.sourceSymbol}`),
        producedBy: PRODUCER_ID,
        producedAt: EPOCH,
        privateKey: keypair.privateKey,
        sourceSymbol: decl.sourceSymbol,
        sourceLayer: decl.sourceLayer,
        targetContractCid: decl.targetContractCid,
        targetLayer: decl.targetLayer,
        ...(decl.notes !== undefined ? { notes: decl.notes } : {}),
      });

      console.log(
        `  bridge   ${decl.name.padEnd(60)}  → ${decl.targetLayer}`,
      );
    }

    mementos.push(memento);
    writeFileSync(
      join(PER_DECL_DIR, `${safeName(decl.name)}.json`),
      JSON.stringify(memento, null, 2),
    );
  }
  console.log();

  // -------------------------------------------------------------------------
  // Step 3: compose all memento CIDs into a catalog root
  // -------------------------------------------------------------------------

  console.log("Step 3: Compose all memento CIDs into a catalog root");
  const inputCids = mementos.map((m) => m.cid).sort();

  const catalogRoot = mintLegacyWitness({
    bindingHash: hash16(`${KIT_NAME}@${KIT_VERSION}`),
    propertyHash: hash16(`catalog-root:${KIT_NAME}@${KIT_VERSION}`),
    verdict: "holds",
    producedBy: PRODUCER_ID,
    producedAt: EPOCH,
    inputCids,
    privateKey: keypair.privateKey,
    rawWitness: JSON.stringify({
      kind: "kit-catalog-root",
      kitName: KIT_NAME,
      kitVersion: KIT_VERSION,
      memberCount: inputCids.length,
      sourceFile: "parseInt.invariant.ts",
      construction: "mint-from-ir",
    }),
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
      sourceFile: "parseInt.invariant.ts",
      construction: "mint-from-ir",
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
  console.log(`  source file → lift (run + collect) → mint → catalog`);
  console.log(`  ${declarations.length} declarations × 1 memento each + 1 catalog root`);
  console.log(`  proofHash = ${catalogRoot.cid}`);
  console.log();
  console.log("This is the kit author's publishing pipeline. Run it on the");
  console.log("kit's invariant source files; out comes the catalog + the");
  console.log("proofHash to put in package.json. Consumers install the");
  console.log("package; their proofkit reads provekit.proofHash; they have");
  console.log("the kit's full memento set locally.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
