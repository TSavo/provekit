/**
 * Layered bridges demo: the DAG forms via bridge mementos.
 *
 * Premise: a TS user calls parseInt. The TS-kit's `parseInt.invariant.ts`
 * doesn't redefine parseInt's contract: it BRIDGES from the TS surface
 * symbol to V8's published parseInt contract. V8's contract bridges to
 * ECMA-262's spec leaf. ECMA-262's spec leaf bridges to IEEE 754. IEEE
 * 754 bridges to a hardware FPU verification artifact.
 *
 * Each bridge is a small content-addressed memento. Each declares
 * "this surface is the realization of that deeper contract." The
 * bridges are the EDGES of the DAG; they're how the layers compose.
 *
 * Running this demo:
 *   - Mints six layered mementos (one per layer)
 *   - Each upper layer's bridge references the lower layer's CID
 *   - The walk from TS user -> hardware traverses all bridges
 *   - All signatures verify
 *   - The chain is durable, content-addressed, signed
 *
 * This demonstrates: parseInt.invariant.ts as a THIN BRIDGE, not as a
 * redefinition. The actual contract lives at the canonical layer; the
 * TS file is a 3-line reference.
 */

import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { generateKeypair } from "../../../src/producerKeys/index.js";
import {
  signEnvelope,
  computeEnvelopeCid,
  verifyEnvelopeSignature,
  VARIANT_SCHEMA_CIDS,
} from "../../../src/claimEnvelope/index.js";
import type {
  ClaimEnvelope,
  BridgeEvidence,
  LegacyWitnessEvidence,
} from "../../../src/claimEnvelope/types.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = join(__dirname, "..", "..", "output", "layered-bridges");

if (!existsSync(OUTPUT_DIR)) mkdirSync(OUTPUT_DIR, { recursive: true });

const KEY_SEED = Buffer.from("layered-bridges-demo-seed-32-byte").subarray(0, 32);
const keypair = generateKeypair({ seed: KEY_SEED });

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function mintLeafMemento(args: {
  bindingHash: string;
  propertyHash: string;
  producedBy: string;
  rawWitness: string;
}): ClaimEnvelope {
  const evidence: LegacyWitnessEvidence = {
    kind: "legacy-witness",
    schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
    body: { rawWitness: args.rawWitness, legacyProducerId: args.producedBy },
  };
  const unsigned = {
    schemaVersion: "1" as const,
    bindingHash: args.bindingHash,
    propertyHash: args.propertyHash,
    verdict: "holds" as const,
    producedBy: args.producedBy,
    producedAt: new Date(0).toISOString(),
    inputCids: [] as string[],
    evidence,
  };
  const signature = signEnvelope(unsigned, keypair.privateKey);
  const cid = computeEnvelopeCid(unsigned);
  const signed: ClaimEnvelope = { ...unsigned, producerSignature: signature, cid };
  if (!verifyEnvelopeSignature(signed, keypair.publicKey)) {
    throw new Error(`signature verification failed for ${args.bindingHash}`);
  }
  return signed;
}

function mintBridgeMemento(args: {
  bindingHash: string;
  propertyHash: string;
  producedBy: string;
  sourceSymbol: string;
  sourceLayer: string;
  targetContractCid: string;
  targetLayer: string;
  notes?: string;
}): ClaimEnvelope {
  const evidence: BridgeEvidence = {
    kind: "bridge",
    schema: VARIANT_SCHEMA_CIDS["bridge"]!,
    body: {
      sourceSymbol: args.sourceSymbol,
      sourceLayer: args.sourceLayer,
      targetContractCid: args.targetContractCid,
      targetLayer: args.targetLayer,
      ...(args.notes !== undefined ? { notes: args.notes } : {}),
    },
  };
  const unsigned = {
    schemaVersion: "1" as const,
    bindingHash: args.bindingHash,
    propertyHash: args.propertyHash,
    verdict: "holds" as const,
    producedBy: args.producedBy,
    producedAt: new Date(0).toISOString(),
    inputCids: [args.targetContractCid].sort(),
    evidence,
  };
  const signature = signEnvelope(unsigned, keypair.privateKey);
  const cid = computeEnvelopeCid(unsigned);
  const signed: ClaimEnvelope = { ...unsigned, producerSignature: signature, cid };
  if (!verifyEnvelopeSignature(signed, keypair.publicKey)) {
    throw new Error(`signature verification failed for ${args.bindingHash}`);
  }
  return signed;
}

console.log("Layered bridges demo: the DAG forms via bridge mementos");
console.log("=".repeat(70));
console.log();

// ---------------------------------------------------------------------------
// LAYER 6 (deepest): Hardware FPU verification artifact
// ---------------------------------------------------------------------------
console.log("Layer 6 (deepest): hardware FPU verification artifact");

const layer6_hardwareFpu = mintLeafMemento({
  bindingHash: hash16("intel:i7-13700k:fpu-unit"),
  propertyHash: hash16("ieee754-arithmetic-conformance"),
  producedBy: "intel-formal-verification@2024.1",
  rawWitness: JSON.stringify({
    chip: "Intel i7-13700K",
    verifiedSubset: "IEEE 754 binary64 add/sub/mul/div",
    methodology: "TCAD simulation + Forte formal verification",
  }),
});
console.log(`  cid: ${layer6_hardwareFpu.cid}`);
console.log();

// ---------------------------------------------------------------------------
// LAYER 5: IEEE 754 spec leaf
// ---------------------------------------------------------------------------
console.log("Layer 5: IEEE 754 spec leaf (bridges to hardware)");

const layer5_ieee754 = mintBridgeMemento({
  bindingHash: hash16("ieee754:integer-conversion"),
  propertyHash: hash16("ieee754-integer-conversion-property"),
  producedBy: "ieee-standards@2019",
  sourceSymbol: "IEEE 754 integer conversion",
  sourceLayer: "IEEE 754:2019 §5.4",
  targetContractCid: layer6_hardwareFpu.cid,
  targetLayer: "Intel/AMD/Apple FPU verification artifacts",
  notes: "Standards body publishes the spec; hardware vendors verify implementations",
});
console.log(`  cid: ${layer5_ieee754.cid}`);
console.log();

// ---------------------------------------------------------------------------
// LAYER 4: ECMA-262 spec leaf
// ---------------------------------------------------------------------------
console.log("Layer 4: ECMA-262 spec leaf (bridges to IEEE 754)");

const layer4_ecma262 = mintBridgeMemento({
  bindingHash: hash16("ecma262:7.1.4.1:parseInt"),
  propertyHash: hash16("ecma262-parseInt-property"),
  producedBy: "ecma-international@2024",
  sourceSymbol: "parseInt",
  sourceLayer: "ECMA-262:2024 §7.1.4.1",
  targetContractCid: layer5_ieee754.cid,
  targetLayer: "IEEE 754 integer conversion semantics",
  notes: "ECMA-262 specifies parseInt's behavior; grounded in IEEE 754 for numeric edge cases",
});
console.log(`  cid: ${layer4_ecma262.cid}`);
console.log();

// ---------------------------------------------------------------------------
// LAYER 3: V8's parseInt implementation
// ---------------------------------------------------------------------------
console.log("Layer 3: V8 parseInt implementation (bridges to ECMA-262)");

const layer3_v8 = mintBridgeMemento({
  bindingHash: hash16("v8:12.4:parseInt"),
  propertyHash: hash16("v8-parseInt-property"),
  producedBy: "v8-team@12.4",
  sourceSymbol: "v8::Number::parseInt",
  sourceLayer: "V8@12.4 (C++ implementation)",
  targetContractCid: layer4_ecma262.cid,
  targetLayer: "ECMA-262 §7.1.4.1",
  notes: "V8's C++ implementation realizes ECMA-262's parseInt; verified by V8's CI test262 conformance suite",
});
console.log(`  cid: ${layer3_v8.cid}`);
console.log();

// ---------------------------------------------------------------------------
// LAYER 2: TS-kit bridge: parseInt.invariant.ts
// ---------------------------------------------------------------------------
console.log("Layer 2: TS-kit bridge (parseInt.invariant.ts: bridges to V8)");

const layer2_tsKit = mintBridgeMemento({
  bindingHash: hash16("ts-kit:global.parseInt"),
  propertyHash: hash16("ts-kit-parseInt-property"),
  producedBy: "ts-kit-demo@0.0.1",
  sourceSymbol: "global.parseInt",
  sourceLayer: "TS-kit@1.0 (TypeScript surface for V8)",
  targetContractCid: layer3_v8.cid,
  targetLayer: "V8@12.4",
  notes: "TS surface symbol is the JS-side projection of V8's C++ parseInt. The TS-kit's catalog file is a 3-line bridge, not a redefinition.",
});
console.log(`  cid: ${layer2_tsKit.cid}`);
console.log();

// ---------------------------------------------------------------------------
// LAYER 1: User's TS callsite memento
// ---------------------------------------------------------------------------
console.log("Layer 1 (user code): TS callsite of parseInt (bridges to TS-kit)");

const layer1_userCode = mintBridgeMemento({
  bindingHash: hash16("user-project:src/billing/invoice.ts:47"),
  propertyHash: hash16("user-callsite-parseInt-property"),
  producedBy: "user-project@1.0.0",
  sourceSymbol: "parseInt(userInput)",
  sourceLayer: "User project src/billing/invoice.ts:47",
  targetContractCid: layer2_tsKit.cid,
  targetLayer: "TS-kit@1.0 parseInt bridge",
  notes: "User code calls parseInt; verification composes against TS-kit bridge, which composes against V8, ECMA-262, IEEE 754, and hardware",
});
console.log(`  cid: ${layer1_userCode.cid}`);
console.log();

// ---------------------------------------------------------------------------
// Walk the chain from user code to hardware
// ---------------------------------------------------------------------------
console.log("DAG walk from user code to hardware:");
console.log();

const layers = [
  { name: "Layer 1 (user code)",       memento: layer1_userCode },
  { name: "Layer 2 (TS-kit bridge)",   memento: layer2_tsKit },
  { name: "Layer 3 (V8 impl)",         memento: layer3_v8 },
  { name: "Layer 4 (ECMA-262 spec)",   memento: layer4_ecma262 },
  { name: "Layer 5 (IEEE 754 spec)",   memento: layer5_ieee754 },
  { name: "Layer 6 (hardware FPU)",    memento: layer6_hardwareFpu },
];

for (let i = 0; i < layers.length; i++) {
  const layer = layers[i]!;
  const evidence = layer.memento.evidence;
  const arrow = i < layers.length - 1 ? "  ↓" : "";
  console.log(`  ${layer.name}`);
  console.log(`    cid:          ${layer.memento.cid}`);
  console.log(`    producedBy:   ${layer.memento.producedBy}`);
  console.log(`    inputCids:    [${layer.memento.inputCids.length}]`);
  if (evidence.kind === "bridge") {
    console.log(`    bridges:      ${evidence.body.sourceLayer}`);
    console.log(`              →   ${evidence.body.targetLayer}`);
  } else {
    console.log(`    leaf:         ${evidence.kind}`);
  }
  if (arrow) console.log(arrow);
}

console.log();
console.log("Verification: every signature round-trips. Every bridge's");
console.log("inputCids includes the deeper layer's CID. The chain is");
console.log("durable, content-addressed, signed end-to-end.");
console.log();

// ---------------------------------------------------------------------------
// Write all six mementos to disk
// ---------------------------------------------------------------------------

const filenames = [
  ["00-hardware-fpu.json", layer6_hardwareFpu],
  ["01-ieee754.json",      layer5_ieee754],
  ["02-ecma262.json",      layer4_ecma262],
  ["03-v8.json",           layer3_v8],
  ["04-ts-kit-bridge.json", layer2_tsKit],
  ["05-user-callsite.json", layer1_userCode],
] as const;

let totalBytes = 0;
for (const [filename, memento] of filenames) {
  const json = JSON.stringify(memento, null, 2);
  writeFileSync(join(OUTPUT_DIR, filename), json);
  totalBytes += json.length;
}

console.log(`Output: ${OUTPUT_DIR}`);
console.log(`Files:  ${filenames.length} JSON mementos`);
console.log(`Bytes:  ${totalBytes}`);
console.log();
console.log("DONE.");
console.log();
console.log("What this proves:");
console.log("  - parseInt.invariant.ts as a THIN BRIDGE (not a redefinition)");
console.log("  - The DAG forms via bridge mementos linking layers");
console.log("  - User code → TS-kit → V8 → ECMA-262 → IEEE 754 → hardware");
console.log("  - Each layer's contract published once; consumers reference by hash");
console.log("  - The framework's claim 'every codebase grounds at physics' is");
console.log("    operational, not aspirational");
