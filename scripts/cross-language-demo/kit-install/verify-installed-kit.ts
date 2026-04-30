/**
 * Consumer-side install flow demo.
 *
 * Pairs with the kit-catalog deliverable (build-ts-kit-catalog.ts).
 * The kit-catalog script demonstrates what a kit AUTHOR publishes to
 * npm. This script demonstrates what a CONSUMER does at install time.
 *
 * Operational story:
 *   1. Consumer runs `pnpm install @provekit/ts-kit`
 *   2. The package's `node_modules/@provekit/ts-kit/` ships:
 *        - dist/ + lib/                    (the kit's runtime code)
 *        - .provekit/ts-kit-catalog.json   (the catalog memento)
 *        - package.json with `provekit` field
 *   3. Consumer's proofkit hooks into the install (post-install or
 *      manual `provekit install-verify`):
 *        a. Reads package.json's `provekit.proofHash` + `provekit.publicKey`
 *        b. Locates the catalog memento at `provekit.catalogPath`
 *        c. Verifies the catalog memento's signature
 *        d. Confirms the catalog's CID matches the published proofHash
 *        e. Enumerates the bridges (catalog.inputCids)
 *        f. Verifies each bridge memento's signature
 *        g. Records the kit as available in the consumer's local
 *           proofkit store (so subsequent compositions can reference
 *           the kit's bridges by CID)
 *
 * NO WALKING. The consumer does NOT traverse into V8's published
 * contracts, ECMA-262 spec leaves, IEEE 754, or hardware. Those are
 * referenced by hash; the consumer trusts the kit author's signature
 * on the bridge claim "this symbol bridges to that CID."
 *
 * Auditing into deeper layers is a downstream tool's job. This script
 * does install-time verification only — local signatures, local CIDs.
 *
 * Run:
 *   npx tsx scripts/cross-language-demo/kit-install/verify-installed-kit.ts
 *
 * It uses the output of build-ts-kit-catalog.ts as the simulated
 * "installed package directory." In production, this would be
 * `node_modules/@provekit/ts-kit/.provekit/`.
 */

import { readFileSync, existsSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createPublicKey } from "node:crypto";
import {
  verifyEnvelopeSignature,
  computeEnvelopeCid,
} from "../../../src/claimEnvelope/index.js";
import type { ClaimEnvelope } from "../../../src/claimEnvelope/types.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SIMULATED_INSTALL_DIR = join(__dirname, "..", "..", "output", "ts-kit-catalog");
const VERIFY_OUTPUT_DIR = join(__dirname, "..", "..", "output", "kit-install-verification");
if (!existsSync(VERIFY_OUTPUT_DIR)) mkdirSync(VERIFY_OUTPUT_DIR, { recursive: true });

console.log("Consumer-side install flow: @provekit/ts-kit");
console.log("=".repeat(70));
console.log();

// ---------------------------------------------------------------------------
// STEP 1: read package.json's provekit metadata
// ---------------------------------------------------------------------------

const pkgFragmentPath = join(SIMULATED_INSTALL_DIR, "package.json.fragment");
if (!existsSync(pkgFragmentPath)) {
  throw new Error(
    `package.json.fragment not found at ${pkgFragmentPath}. ` +
    `Run scripts/cross-language-demo/kit-catalog/build-ts-kit-catalog.ts first.`,
  );
}

const packageJson = JSON.parse(readFileSync(pkgFragmentPath, "utf8"));
const provekitMeta = packageJson.provekit;

if (!provekitMeta || !provekitMeta.proofHash || !provekitMeta.publicKey) {
  throw new Error("package.json missing required `provekit` fields (proofHash, publicKey)");
}

console.log("Step 1: Read package.json provekit metadata");
console.log(`  package:   ${packageJson.name}@${packageJson.version}`);
console.log(`  proofHash: ${provekitMeta.proofHash}`);
console.log(`  catalogPath: ${provekitMeta.catalogPath}`);
console.log(`  publicKey: ${provekitMeta.publicKey.slice(0, 32)}... (${provekitMeta.publicKey.length} chars)`);
console.log();

// ---------------------------------------------------------------------------
// STEP 2: load the catalog memento
// ---------------------------------------------------------------------------

const catalogPath = join(SIMULATED_INSTALL_DIR, "ts-kit-catalog.json");
if (!existsSync(catalogPath)) {
  throw new Error(`Catalog memento not found at ${catalogPath}`);
}

const catalog: ClaimEnvelope = JSON.parse(readFileSync(catalogPath, "utf8"));

console.log("Step 2: Load catalog memento");
console.log(`  cid:         ${catalog.cid}`);
console.log(`  bindingHash: ${catalog.bindingHash}`);
console.log(`  producedBy:  ${catalog.producedBy}`);
console.log(`  inputCids:   ${catalog.inputCids.length} (the kit's bridges)`);
console.log();

// ---------------------------------------------------------------------------
// STEP 3: verify the catalog's CID matches the published proofHash
// ---------------------------------------------------------------------------

console.log("Step 3: Verify catalog CID matches published proofHash");
if (catalog.cid !== provekitMeta.proofHash) {
  console.error(`  ✗ MISMATCH: catalog.cid (${catalog.cid}) != proofHash (${provekitMeta.proofHash})`);
  process.exit(1);
}
console.log(`  ✓ catalog.cid === proofHash (${catalog.cid})`);
console.log();

// ---------------------------------------------------------------------------
// STEP 4: recompute the catalog's CID from its bytes (defense against
// tampering: a tampered catalog would have a different CID even if its
// `cid` field still claims the published value)
// ---------------------------------------------------------------------------

console.log("Step 4: Recompute catalog CID from canonical bytes");
const { cid: _ignored, producerSignature: _ignored2, ...unsigned } = catalog;
const recomputed = computeEnvelopeCid(unsigned);
if (recomputed !== catalog.cid) {
  console.error(`  ✗ TAMPERING DETECTED: bytes hash to ${recomputed}, claimed cid ${catalog.cid}`);
  process.exit(1);
}
console.log(`  ✓ recomputed CID matches stored cid (${recomputed})`);
console.log();

// ---------------------------------------------------------------------------
// STEP 5: verify the catalog's signature against the published public key
// ---------------------------------------------------------------------------

console.log("Step 5: Verify catalog signature against published public key");
const publicKeyDer = Buffer.from(provekitMeta.publicKey, "base64");
const publicKey = createPublicKey({ key: publicKeyDer, format: "der", type: "spki" });

if (!verifyEnvelopeSignature(catalog, publicKey)) {
  console.error("  ✗ SIGNATURE INVALID");
  process.exit(1);
}
console.log(`  ✓ catalog signature valid (signed by ${catalog.producedBy})`);
console.log();

// ---------------------------------------------------------------------------
// STEP 6: enumerate and verify each bridge memento
// ---------------------------------------------------------------------------

console.log("Step 6: Verify each bridge memento referenced by catalog.inputCids");
const bridgesDir = join(SIMULATED_INSTALL_DIR, "bridges");
const bridgeFiles = readdirSync(bridgesDir).filter((f) => f.endsWith(".json"));

const bridgesByCid = new Map<string, ClaimEnvelope>();
for (const filename of bridgeFiles) {
  const bridge: ClaimEnvelope = JSON.parse(readFileSync(join(bridgesDir, filename), "utf8"));
  bridgesByCid.set(bridge.cid, bridge);
}

let bridgesVerified = 0;
let bridgesMissing = 0;
let bridgesBadSignature = 0;

for (const expectedCid of catalog.inputCids) {
  const bridge = bridgesByCid.get(expectedCid);
  if (!bridge) {
    console.error(`  ✗ MISSING bridge memento for cid ${expectedCid}`);
    bridgesMissing++;
    continue;
  }
  if (!verifyEnvelopeSignature(bridge, publicKey)) {
    console.error(`  ✗ BAD SIGNATURE on bridge ${expectedCid}`);
    bridgesBadSignature++;
    continue;
  }
  if (bridge.evidence.kind === "bridge") {
    console.log(`  ✓ ${bridge.evidence.body.sourceSymbol.padEnd(28)} cid: ${bridge.cid}`);
  } else {
    console.log(`  ✓ ${bridge.cid} (non-bridge memento)`);
  }
  bridgesVerified++;
}

console.log();
console.log(`  Verified: ${bridgesVerified}/${catalog.inputCids.length} bridges`);
if (bridgesMissing > 0) console.log(`  Missing:  ${bridgesMissing}`);
if (bridgesBadSignature > 0) console.log(`  Bad sigs: ${bridgesBadSignature}`);
console.log();

// ---------------------------------------------------------------------------
// STEP 7: produce the install-verification memento
//
// This memento records the local proofkit's verification of the kit
// install. It is itself a leaf in the consumer's local DAG; downstream
// composition (consumer's own .invariant.ts files referencing kit
// symbols) will reference it.
// ---------------------------------------------------------------------------

console.log("Step 7: Record install verification");

const installVerification = {
  consumer: "consumer-project@local",
  installedPackage: `${packageJson.name}@${packageJson.version}`,
  catalogCid: catalog.cid,
  publishedProofHash: provekitMeta.proofHash,
  bridgesAvailable: catalog.inputCids,
  verifiedAt: new Date(0).toISOString(),
  verdict: bridgesMissing === 0 && bridgesBadSignature === 0 ? "holds" : "violated",
  signatureValidated: true,
  cidMatchesProofHash: true,
  recomputedCidMatches: true,
};

writeFileSync(
  join(VERIFY_OUTPUT_DIR, "install-verification.json"),
  JSON.stringify(installVerification, null, 2),
);

console.log(`  Verdict: ${installVerification.verdict}`);
console.log(`  Output:  ${join(VERIFY_OUTPUT_DIR, "install-verification.json")}`);
console.log();

// ---------------------------------------------------------------------------
// SUMMARY
// ---------------------------------------------------------------------------

console.log("=".repeat(70));
if (installVerification.verdict === "holds") {
  console.log(`✓ Kit ${packageJson.name}@${packageJson.version} installed and verified`);
  console.log(`  proofHash:        ${catalog.cid}`);
  console.log(`  bridges available: ${bridgesVerified}`);
  console.log(`  signed by:        ${catalog.producedBy}`);
  console.log();
  console.log("The consumer's project can now compose against this kit's bridges.");
  console.log("The consumer's local DAG references each bridge by CID. No walking");
  console.log("into V8 / ECMA-262 / IEEE / hardware was needed — those are bridge");
  console.log("targets referenced by hash, audited only if the consumer's policy");
  console.log("demands it.");
} else {
  console.log(`✗ Install verification failed`);
  process.exit(1);
}
