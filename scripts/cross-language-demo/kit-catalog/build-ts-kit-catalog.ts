/**
 * Kit catalog deliverable demo.
 *
 * What this script produces is what a kit author publishes to npm:
 * a single signed catalog memento whose CID is the kit's `proofHash`.
 *
 *   package.json: {
 *     "name": "@provekit/ts-kit",
 *     "version": "1.0.0",
 *     "provekit": {
 *       "proofHash": "<the CID this script produces>",
 *       "kitVersion": "ts-kit@1.0",
 *       "publicKey": "<ed25519 spki public key, base64>"
 *     }
 *   }
 *
 * When a consumer runs `pnpm install @provekit/ts-kit`, their proofkit:
 *   1. Reads `package.json`'s `provekit.proofHash`
 *   2. Locates the corresponding catalog memento JSON in the package
 *   3. Verifies the signature against `provekit.publicKey`
 *   4. Walks the catalog's `inputCids` to discover the kit's bridge
 *      mementos (parseInt → V8, Math.abs → V8, etc.)
 *   5. Composes the consumer's project DAG against the kit's catalog
 *      root by hash
 *
 * The kit catalog is the kit's primary deliverable. Everything else
 * (lifter binaries, source files, type declarations) is operational
 * scaffolding. The catalog's CID is what gets pinned, what gets
 * referenced, what travels through npm. The catalog IS the kit's
 * identity.
 *
 * Run: npx tsx scripts/cross-language-demo/kit-catalog/build-ts-kit-catalog.ts
 *
 * Output:
 *   - scripts/output/ts-kit-catalog/ts-kit-catalog.json (the root memento)
 *   - scripts/output/ts-kit-catalog/bridges/*.json (per-built-in bridges)
 *   - scripts/output/ts-kit-catalog/package.json.fragment (the provekit field)
 *   - scripts/output/ts-kit-catalog/public-key.b64 (for consumer verification)
 */

import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { generateKeypair } from "../../../implementations/typescript/src/producerKeys/index.js";
import {
  mintBridge,
  mintMemento,
  VARIANT_SCHEMA_CIDS,
} from "../../../implementations/typescript/src/claimEnvelope/index.js";
import type { ClaimEnvelope } from "../../../implementations/typescript/src/claimEnvelope/types.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = join(__dirname, "..", "..", "output", "ts-kit-catalog");
const BRIDGES_DIR = join(OUTPUT_DIR, "bridges");
if (!existsSync(BRIDGES_DIR)) mkdirSync(BRIDGES_DIR, { recursive: true });

const KIT_NAME = "@provekit/ts-kit";
const KIT_VERSION = "1.0.0";
const KIT_PRODUCER_ID = "ts-kit@1.0";
const EPOCH = new Date(0).toISOString();

const KEY_SEED = Buffer.from("ts-kit-1.0-publishing-key-seed!!").subarray(0, 32);
const keypair = generateKeypair({ seed: KEY_SEED });

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

console.log(`Building kit catalog: ${KIT_NAME}@${KIT_VERSION}`);
console.log("=".repeat(70));
console.log();

// ---------------------------------------------------------------------------
// THE KIT'S BRIDGES
//
// Each bridge: a TS surface symbol -> a deeper-layer published contract.
// The deeper contracts are referenced by CID; we don't redefine them.
//
// In a real kit catalog these CIDs would be V8's, ECMA-262's, IEEE's
// actual published mementos. Here we use placeholder CIDs to demonstrate
// the catalog shape; the value flows when those upstream layers ship
// real contracts.
// ---------------------------------------------------------------------------

interface BridgeSpec {
  symbol: string;
  targetLayer: string;
  targetContractCid: string; // would be V8/ECMA-262/etc. in production
  notes?: string;
}

const PLACEHOLDER_V8_CID = "0".repeat(32);
const PLACEHOLDER_ECMA_CID = "1".repeat(32);

const BRIDGES: BridgeSpec[] = [
  {
    symbol: "global.parseInt",
    targetLayer: "V8@12.4 parseInt (placeholder; real CID when V8 publishes)",
    targetContractCid: PLACEHOLDER_V8_CID,
    notes: "Bridges to V8's published parseInt contract",
  },
  {
    symbol: "global.parseFloat",
    targetLayer: "V8@12.4 parseFloat",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "global.isNaN",
    targetLayer: "V8@12.4 isNaN",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "global.isFinite",
    targetLayer: "V8@12.4 isFinite",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "Math.abs",
    targetLayer: "V8@12.4 Math.abs (grounded in IEEE 754)",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "Math.max",
    targetLayer: "V8@12.4 Math.max",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "Math.min",
    targetLayer: "V8@12.4 Math.min",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "Math.floor",
    targetLayer: "V8@12.4 Math.floor",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "Math.ceil",
    targetLayer: "V8@12.4 Math.ceil",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "Math.sqrt",
    targetLayer: "V8@12.4 Math.sqrt (IEEE 754 sqrt)",
    targetContractCid: PLACEHOLDER_V8_CID,
  },
  {
    symbol: "Number.isInteger",
    targetLayer: "ECMA-262 Number.isInteger",
    targetContractCid: PLACEHOLDER_ECMA_CID,
  },
  {
    symbol: "Number.isFinite",
    targetLayer: "ECMA-262 Number.isFinite",
    targetContractCid: PLACEHOLDER_ECMA_CID,
  },
  {
    symbol: "Number.isNaN",
    targetLayer: "ECMA-262 Number.isNaN",
    targetContractCid: PLACEHOLDER_ECMA_CID,
  },
  {
    symbol: "Array.prototype.length",
    targetLayer: "ECMA-262 Array.prototype.length",
    targetContractCid: PLACEHOLDER_ECMA_CID,
  },
  {
    symbol: "Array.prototype.includes",
    targetLayer: "ECMA-262 Array.prototype.includes",
    targetContractCid: PLACEHOLDER_ECMA_CID,
  },
  {
    symbol: "String.prototype.length",
    targetLayer: "ECMA-262 String.prototype.length",
    targetContractCid: PLACEHOLDER_ECMA_CID,
  },
  {
    symbol: "String.prototype.includes",
    targetLayer: "ECMA-262 String.prototype.includes",
    targetContractCid: PLACEHOLDER_ECMA_CID,
  },
];

// ---------------------------------------------------------------------------
// MINT EACH BRIDGE
// ---------------------------------------------------------------------------

console.log(`Minting ${BRIDGES.length} bridge mementos...`);
console.log();

const bridgeMementos: ClaimEnvelope[] = [];
for (const spec of BRIDGES) {
  const memento = mintBridge({
    bindingHash: hash16(`${KIT_PRODUCER_ID}:${spec.symbol}`),
    propertyHash: hash16(`bridge:${spec.symbol}`),
    producedBy: KIT_PRODUCER_ID,
    producedAt: EPOCH,
    privateKey: keypair.privateKey,
    sourceSymbol: spec.symbol,
    sourceLayer: KIT_PRODUCER_ID,
    targetContractCid: spec.targetContractCid,
    targetLayer: spec.targetLayer,
    ...(spec.notes !== undefined ? { notes: spec.notes } : {}),
  });
  bridgeMementos.push(memento);

  const safeName = spec.symbol.replace(/\W/g, "_");
  writeFileSync(
    join(BRIDGES_DIR, `${safeName}.json`),
    JSON.stringify(memento, null, 2),
  );
  console.log(`  ✓ ${spec.symbol.padEnd(28)} cid: ${memento.cid}`);
}
console.log();

// ---------------------------------------------------------------------------
// COMPOSE THE CATALOG ROOT
//
// The catalog root memento composes all bridges as inputCids. Its CID is
// the kit's proofHash: the single 32-character hex value that goes in
// package.json's provekit.proofHash field.
// ---------------------------------------------------------------------------

const catalogInputCids = bridgeMementos.map((m) => m.cid).sort();

const catalogRoot = mintMemento({
  bindingHash: hash16(`${KIT_NAME}@${KIT_VERSION}`),
  propertyHash: hash16(`kit-catalog-root:${KIT_NAME}@${KIT_VERSION}`),
  verdict: "holds",
  producedBy: KIT_PRODUCER_ID,
  producedAt: EPOCH,
  inputCids: catalogInputCids,
  evidence: {
    kind: "legacy-witness",
    schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
    body: {
      rawWitness: JSON.stringify({
        kind: "kit-catalog",
        kitName: KIT_NAME,
        kitVersion: KIT_VERSION,
        bridgeCount: bridgeMementos.length,
        bridgeCids: catalogInputCids,
      }),
      legacyProducerId: KIT_PRODUCER_ID,
    },
  },
  privateKey: keypair.privateKey,
});

console.log(`Catalog root memento minted:`);
console.log(`  bridges:     ${bridgeMementos.length}`);
console.log(`  inputCids:   ${catalogInputCids.length} (sorted)`);
console.log(`  bindingHash: ${catalogRoot.bindingHash}`);
console.log(`  proofHash:   ${catalogRoot.cid}`);  // THE proofHash for package.json
console.log();

writeFileSync(
  join(OUTPUT_DIR, "ts-kit-catalog.json"),
  JSON.stringify(catalogRoot, null, 2),
);

// ---------------------------------------------------------------------------
// EMIT THE PACKAGE.JSON FRAGMENT THE KIT AUTHOR PUBLISHES TO NPM
// ---------------------------------------------------------------------------

// SPKI public key as base64 for consumer-side verification.
const publicKeyB64 = keypair.publicKey
  .export({ type: "spki", format: "der" })
  .toString("base64");

writeFileSync(join(OUTPUT_DIR, "public-key.b64"), publicKeyB64 + "\n");

const packageFragment = {
  name: KIT_NAME,
  version: KIT_VERSION,
  description: "The TypeScript kit for ProvekIt: built-in symbol bridges to V8 / ECMA-262 / IEEE 754 / hardware",
  files: [
    "dist/",
    "lib/",
    ".provekit/",
  ],
  provekit: {
    proofHash: catalogRoot.cid,
    catalogPath: ".provekit/ts-kit-catalog.json",
    kitVersion: KIT_PRODUCER_ID,
    publicKey: publicKeyB64,
  },
};

writeFileSync(
  join(OUTPUT_DIR, "package.json.fragment"),
  JSON.stringify(packageFragment, null, 2) + "\n",
);

// ---------------------------------------------------------------------------
// DONE
// ---------------------------------------------------------------------------

console.log("Output written:");
console.log(`  ${OUTPUT_DIR}/`);
console.log(`    ts-kit-catalog.json          (the catalog root memento)`);
console.log(`    package.json.fragment        (what kit author adds to package.json)`);
console.log(`    public-key.b64               (kit author's ed25519 SPKI public key)`);
console.log(`    bridges/*.json               (${bridgeMementos.length} per-bridge mementos)`);
console.log();
console.log("What a consumer does at `pnpm install @provekit/ts-kit`:");
console.log("  1. Reads package.json's provekit.proofHash field");
console.log(`     → ${catalogRoot.cid}`);
console.log("  2. Locates .provekit/ts-kit-catalog.json in the installed package");
console.log("  3. Verifies the catalog signature against provekit.publicKey");
console.log("  4. Walks catalog.inputCids to discover the kit's bridge mementos");
console.log("  5. Composes the consumer's project DAG against the catalog root");
console.log();
console.log("The catalog's CID IS the kit's identity in the proof substrate.");
console.log("`@provekit/ts-kit@1.0.0+contentHash+proofHash` is the three-coordinate");
console.log("artifact identity. proofHash =", catalogRoot.cid);
