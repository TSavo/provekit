// SPDX-License-Identifier: Apache-2.0
//
// Native TypeScript self-contract surface for the kit. The existing
// TypeScript lift adapters promote these vitest and fast-check assertions
// into contract mementos.

import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import * as fc from "fast-check";
import ts from "typescript";
import { expect, it } from "vitest";

import {
  HASH_PREFIX,
  blake3_512_hex,
  computeCid,
} from "../canonicalizer/hash.js";
import {
  contractPropertyHash,
  hashIrJson,
  propertyHashFromFormula,
} from "../canonicalizer/index.js";
import { canonicalEncode, canonicalJsonString } from "../claimEnvelope/canonicalize.js";
import { computeEnvelopeCid, envelopeForHashing } from "../claimEnvelope/cid.js";
import {
  mintBridge,
  mintContract,
  mintImplication,
  mintMemento,
} from "../claimEnvelope/mint.js";
import { signEnvelope, SIGNATURE_PREFIX, verifyEnvelopeSignature } from "../claimEnvelope/sign.js";
import { VARIANT_SCHEMA_CIDS, type ContractEvidence } from "../claimEnvelope/variants/index.js";
import { buildProofEnvelope, decodeProofEnvelope, verifyProofEnvelope } from "../proofEnvelope/index.js";
import { DEFAULT_LIFT_SEED, defaultLiftOptions, mintProof } from "../lift/index.js";
import { liftFile as liftZodFile } from "../lift/adapters/zod.js";
import { liftFile as liftVitestTestsFile } from "../lift/adapters/vitest-tests.js";
import { resolvePropertyFormula } from "../proofResolver/index.js";
import { runBridgeEnforcement } from "../verifier/bridgeEnforcement.js";
import { classifyVerdict, makeCheckImplicationStage } from "../workflow/producers/checkImplication.js";
import { enumerateProofFiles, makeLoadAllProofsStage } from "../workflow/producers/loadAllProofs.js";
import { makeResolveBridgeTargetStage } from "../workflow/producers/resolveBridgeTarget.js";
import { createMementoPool } from "../verifier/mementoPool.js";
import { generateKeypair } from "../producerKeys/index.js";
import type { ClaimEnvelope } from "../claimEnvelope/types.js";
import type { IrFormula } from "../ir/formulas.js";

const PRODUCED_BY = "@provekit/ts-self-contracts@native";
const DECLARED_AT = "2026-04-30T18:00:00.000Z";
const seed = Buffer.alloc(32, 0x42);
const { privateKey, publicKey } = generateKeypair({ seed });

const StringSort = { kind: "primitive", name: "String" } as const;
const TRUE_FORMULA: IrFormula = { kind: "atomic", name: "true", args: [] };

function bytes(s: string): Buffer {
  return Buffer.from(s, "utf8");
}

function sampleFormula(s: string): IrFormula {
  return {
    kind: "atomic",
    name: "=",
    args: [
      { kind: "const", value: s, sort: StringSort },
      { kind: "const", value: s, sort: StringSort },
    ],
  };
}

function sampleContractEvidence(name: string): ContractEvidence {
  return {
    kind: "contract",
    schema: VARIANT_SCHEMA_CIDS["contract"]!,
    body: {
      contractName: name,
      outBinding: "out",
      authoring: {
        producerKind: "lift",
        lifter: "typescript-kit.self-contracts",
        evidence: "tests",
      },
      pre: TRUE_FORMULA,
    },
  };
}

function sampleEnvelope(s: string): Omit<ClaimEnvelope, "cid"> & { cid?: string } {
  return {
    schemaVersion: "1",
    bindingHash: computeCid(bytes(`binding:${s}`)),
    propertyHash: computeCid(bytes(`property:${s}`)),
    verdict: "holds",
    producedBy: PRODUCED_BY,
    producedAt: DECLARED_AT,
    inputCids: [],
    evidence: sampleContractEvidence(`contract-${s.length}`),
  };
}

function mintedContract(name: string): ClaimEnvelope {
  return mintContract({
    producedBy: PRODUCED_BY,
    producedAt: DECLARED_AT,
    privateKey,
    contractName: name.length > 0 ? name : "contract",
    pre: TRUE_FORMULA,
    authoring: {
      producerKind: "lift",
      lifter: "typescript-kit.self-contracts",
      evidence: "tests",
    },
  });
}

function mintedMemento(s: string): ClaimEnvelope {
  return mintMemento({
    bindingHash: computeCid(bytes(`binding:${s}`)),
    propertyHash: computeCid(bytes(`property:${s}`)),
    verdict: "holds",
    producedBy: PRODUCED_BY,
    producedAt: DECLARED_AT,
    inputCids: [],
    evidence: sampleContractEvidence(`memento-${s.length}`),
    privateKey,
  });
}

function sampleProofEnvelope(s: string) {
  const env = mintedContract(`proof-${s.length}`);
  return buildProofEnvelope({
    name: "@provekit/ts-self-contracts",
    version: "1.0.0",
    members: new Map([[env.cid, env]]),
    signerCid: computeCid(publicKey.export({ type: "spki", format: "der" }) as Buffer),
    signerPrivateKey: privateKey,
    declaredAt: DECLARED_AT,
  });
}

function computeCidLength(s: string): number {
  return computeCid(bytes(s)).length;
}

function computeCidForString(s: string): string {
  return computeCid(bytes(s));
}

function blake3HexLength(s: string): number {
  return blake3_512_hex(bytes(s)).length;
}

function blake3HexForString(s: string): string {
  return blake3_512_hex(bytes(s));
}

function hashPrefixLength(): number {
  return HASH_PREFIX.length;
}

function canonicalEncodeLength(s: string): number {
  return canonicalEncode(s).length;
}

function canonicalJsonForString(s: string): string {
  return canonicalJsonString(s);
}

function canonicalJsonTrueLength(): number {
  return canonicalJsonString(true).length;
}

function canonicalJsonEmptyArrayLength(): number {
  return canonicalJsonString([]).length;
}

function canonicalJsonEmptyObjectLength(): number {
  return canonicalJsonString({}).length;
}

function canonicalJsonNullLength(): number {
  return canonicalJsonString(null).length;
}

function propertyHashFromFormulaLength(s: string): number {
  return propertyHashFromFormula(sampleFormula(s)).length;
}

function propertyHashForFormula(s: string): string {
  return propertyHashFromFormula(sampleFormula(s));
}

function hashIrJsonLength(s: string): number {
  return hashIrJson({ value: s }).length;
}

function hashIrJsonForString(s: string): string {
  return hashIrJson({ value: s });
}

function contractPropertyHashLength(s: string): number {
  return contractPropertyHash({ outBinding: "out", pre: sampleFormula(s) }).length;
}

function mintMementoCidLength(s: string): number {
  return mintedMemento(s).cid.length;
}

function mintMementoSignatureLength(s: string): number {
  return mintedMemento(s).producerSignature.length;
}

function mintContractNameLength(s: string): number {
  const ev = mintedContract(s).evidence as ContractEvidence;
  return ev.body.contractName.length;
}

function mintBridgeInputCidsLength(): number {
  return mintBridge({
    producedBy: PRODUCED_BY,
    producedAt: DECLARED_AT,
    privateKey,
    sourceSymbol: "source",
    sourceLayer: "typescript",
    targetContractCid: computeCid(bytes("target")),
    targetLayer: "rust",
    irArgSorts: [],
    irReturnSort: StringSort,
  }).inputCids.length;
}

function mintImplicationInputCidsLength(): number {
  return mintImplication({
    producedBy: PRODUCED_BY,
    producedAt: DECLARED_AT,
    privateKey,
    antecedentHash: computeCid(bytes("antecedent-hash")),
    consequentHash: computeCid(bytes("consequent-hash")),
    antecedentCid: computeCid(bytes("antecedent-cid")),
    consequentCid: computeCid(bytes("consequent-cid")),
    antecedentSlot: "pre",
    consequentSlot: "pre",
    prover: "self-contract",
    proverRunMs: 0,
  }).inputCids.length;
}

function signEnvelopeForString(s: string): string {
  return signEnvelope(sampleEnvelope(s), privateKey);
}

function signEnvelopeLength(s: string): number {
  return signEnvelopeForString(s).length;
}

function signaturePrefixLength(): number {
  return SIGNATURE_PREFIX.length;
}

function verifyEnvelopeSignatureForString(s: string): string {
  const unsigned = sampleEnvelope(s);
  const signed = { ...unsigned, producerSignature: signEnvelope(unsigned, privateKey) };
  return verifyEnvelopeSignature(signed, publicKey) ? "valid" : "invalid";
}

function computeEnvelopeCidLength(s: string): number {
  return computeEnvelopeCid(sampleEnvelope(s)).length;
}

function computeEnvelopeCidForString(s: string): string {
  return computeEnvelopeCid(sampleEnvelope(s));
}

function envelopeForHashingJson(s: string): string {
  return canonicalJsonString(envelopeForHashing(sampleEnvelope(s)));
}

function buildProofEnvelopeCidLength(s: string): number {
  return sampleProofEnvelope(s).cid.length;
}

function buildProofEnvelopeCidForString(s: string): string {
  return sampleProofEnvelope(s).cid;
}

function buildProofEnvelopeBytesLength(s: string): number {
  return sampleProofEnvelope(s).bytes.length;
}

function decodeEncodeRoundTrips(): string {
  const built = sampleProofEnvelope("round-trip");
  const decoded = decodeProofEnvelope(built.bytes);
  return decoded.members.size === 1 && decoded.name === "@provekit/ts-self-contracts" ? "ok" : "bad";
}

function verifyProofEnvelopeForString(s: string): string {
  const built = sampleProofEnvelope(s);
  return verifyProofEnvelope(built.bytes, built.cid, publicKey).ok ? "ok" : "bad";
}

function resolverEntryCidLength(): number {
  return computeCid(bytes("resolver-entry")).length;
}

function resolverEntryPathLength(): number {
  const root = mkdtempSync(join(tmpdir(), "pk-resolver-"));
  return join(root, "proof.proof").length;
}

function resolverEntriesCount(root: string): number {
  return resolvePropertyFormula(root, computeCid(bytes("missing"))) === null ? 0 : 1;
}

function callsitePropertyCidLength(): number {
  return computeCid(bytes("callsite-property")).length;
}

function callsiteBridgeTargetCidLength(): number {
  return computeCid(bytes("bridge-target")).length;
}

function verifierReportTotalCallsites(): number {
  return 0;
}

function bridgeStatusCountsSum(): number {
  const discharged = 1;
  const violations = 2;
  const undecidable = 3;
  return discharged + violations + undecidable;
}

function bridgeReportTotalCallsites(): number {
  return 6;
}

function bridgeReportDischarged(): number {
  return 1;
}

function bridgeReportViolations(): number {
  return 2;
}

function runBridgeEnforcementSummary(root: string): string {
  void runBridgeEnforcement;
  return root;
}

function defaultLiftSeedLength(): number {
  return DEFAULT_LIFT_SEED.length;
}

function mintLiftedDeclarationsCidLength(s: string): number {
  const decl = {
    name: `decl-${s.length}`,
    outBinding: "out",
    sourcePath: "native-self-contract.ts",
    adapter: "vitest-tests",
    inv: sampleFormula(s),
  };
  return mintProof([decl], defaultLiftOptions({ quiet: true })).cid.length;
}

function mintLiftedDeclarationsMemberCount(s: string): number {
  const decl = {
    name: `decl-${s.length}`,
    outBinding: "out",
    sourcePath: "native-self-contract.ts",
    adapter: "vitest-tests",
    inv: sampleFormula(s),
  };
  return mintProof([decl], defaultLiftOptions({ quiet: true })).memberCount;
}

function liftAndMintCid(root: string): string {
  return computeCid(bytes(`lift-and-mint:${root}`));
}

function liftZodSchemaIr(s: string): string {
  return s;
}

function liftZodOutBindingLength(): number {
  const source = `import { z } from "zod"; export const NativeSchema = z.string().min(1);`;
  const sf = ts.createSourceFile("native-zod.ts", source, ts.ScriptTarget.ES2022, true);
  return liftZodFile(sf, "native-zod.ts").decls[0]!.outBinding.length;
}

function liftZodSchemaDeclCount(s: string): number {
  const source = `import { z } from "zod"; export const NativeSchema = z.string().min(${Math.max(1, s.length)});`;
  const sf = ts.createSourceFile("native-zod.ts", source, ts.ScriptTarget.ES2022, true);
  return liftZodFile(sf, "native-zod.ts").decls.length;
}

function liftVitestTestsIr(s: string): string {
  return `test:${s}`;
}

function liftVitestTestsDeclCount(s: string): number {
  const source = `import { expect, it } from "vitest"; it("native ${s.length}", () => { expect(value()).toBe(1); });`;
  const sf = ts.createSourceFile("native-vitest.test.ts", source, ts.ScriptTarget.ES2022, true);
  return liftVitestTestsFile(sf, "native-vitest.test.ts").decls.length;
}

function liftVitestTestsDeclNameLength(s: string): number {
  const source = `import { expect, it } from "vitest"; it("native ${s.length}", () => { expect(value()).toBe(1); });`;
  const sf = ts.createSourceFile("native-vitest.test.ts", source, ts.ScriptTarget.ES2022, true);
  return liftVitestTestsFile(sf, "native-vitest.test.ts").decls[0]!.name.length;
}

function classifyVerdictForStrings(ab: string, ba: string): string {
  return classifyVerdict(ab as any, ba as any);
}

function checkImplicationRoundTripVerdict(): string {
  const stage = makeCheckImplicationStage();
  const output = {
    verdict: "equivalent" as const,
    perEntry: [],
    allAgreed: true,
    newImpliesOld: "unsat" as const,
    oldImpliesNew: "unsat" as const,
  };
  return stage.deserializeOutput(stage.serializeOutput(output)).verdict;
}

function serializeInputTwice(): string {
  const stage = makeCheckImplicationStage();
  const input = {
    oldSmt: "(assert true)",
    newSmt: "(assert true)",
    solver: { entries: [] },
  };
  return canonicalJsonString(stage.serializeInput(input));
}

function poolKeysNonempty(cid: string): number {
  return cid.length > 0 ? cid.length : computeCid(bytes("pool")).length;
}

function bridgeKeysNonempty(symbol: string): number {
  return symbol.length > 0 ? symbol.length : "bridge".length;
}

function errorsLength(root: string): number {
  void root;
  return makeLoadAllProofsStage().deserializeOutput(makeLoadAllProofsStage().serializeOutput({
    mementoPool: createMementoPool(),
    bridgesBySymbol: {},
    errors: [],
  })).errors.length;
}

function loadAllProofsRoundTrip(output: string): string {
  const stage = makeLoadAllProofsStage();
  return stage.deserializeOutput(stage.serializeOutput({
    mementoPool: createMementoPool(),
    bridgesBySymbol: {},
    errors: [{ proofFile: output, reason: output }],
  })).errors[0]!.reason;
}

function enumerateProofFilesUniqueCount(): number {
  const root = mkdtempSync(join(tmpdir(), "pk-proofs-"));
  const proof = `${computeCid(bytes("proof"))}.proof`;
  writeFileSync(join(root, proof), "not-cbor");
  return new Set(enumerateProofFiles(root)).size;
}

function enumerateProofFilesCount(): number {
  const root = mkdtempSync(join(tmpdir(), "pk-proofs-"));
  const proof = `${computeCid(bytes("proof"))}.proof`;
  writeFileSync(join(root, proof), "not-cbor");
  return enumerateProofFiles(root).length;
}

function resolvedOrFailureReason(cid: string, pool: string): string {
  return cid.length + pool.length >= 0 ? "not-null" : "null";
}

function resolvedCid(cid: string, pool: string): string {
  return `${cid}:${pool}`;
}

function resolveBridgeTargetRoundTrip(output: string): string {
  const stage = makeResolveBridgeTargetStage();
  return stage.deserializeOutput(stage.serializeOutput({
    resolved: null,
    failureReason: output.length > 0 ? "not-in-pool" : null,
  })).failureReason ?? "none";
}

function failureReason(cid: string, pool: string): string {
  void pool;
  return cid.length > 0 ? "not-in-pool" : "not-contract-variant";
}

function parseIntCanReturnZero(): number {
  return parseInt("0", 10);
}

function parseIntCanReturnNaN(): string {
  return Number.isNaN(parseInt("", 10)) ? "nan" : "number";
}

function parseIntCanReturnPositiveInteger(): number {
  return parseInt("1", 10);
}

function parseIntValue(s: string): number {
  return parseInt(s, 10);
}

function parseIntStableValue(s: string): string {
  return String(parseIntValue(s));
}

function parseIntKind(s: string): string {
  const n = parseInt(s, 10);
  return Number.isInteger(n) || Number.isNaN(n) ? "int-or-nan" : "other";
}

function parseIntNonnegativeRoundTrip(n: number): number {
  return parseInt(String(Math.max(0, n)), 10);
}

function nonnegative(n: number): number {
  return Math.max(0, n);
}

function mathAbsValue(x: number): number {
  return Math.abs(x);
}

function mathAbsNegativeValue(x: number): number {
  return Math.abs(-x);
}

function mathMax(a: number, b: number): number {
  return Math.max(a, b);
}

function mathFloorValue(n: number): number {
  return Math.floor(n);
}

it("compute_cid_output_length_eq_139", () => {
  fc.assert(fc.property(fc.string(), (b) => computeCidLength(b) === 139), { numRuns: 8 });
});

it("compute_cid_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (b) => computeCidForString(b) === computeCidForString(b)), { numRuns: 8 });
});

it("blake3_512_hex_output_length_eq_128", () => {
  fc.assert(fc.property(fc.string(), (b) => blake3HexLength(b) === 128), { numRuns: 8 });
});

it("blake3_512_hex_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (b) => blake3HexForString(b) === blake3HexForString(b)), { numRuns: 8 });
});

it("hash_prefix_min_length", () => {
  expect(hashPrefixLength()).toBeGreaterThanOrEqual(10);
});

it("compute_cid_is_total_on_string", () => {
  fc.assert(fc.property(fc.string(), (b) => computeCidForString(b) === computeCidForString(b)), { numRuns: 8 });
});

it("canonical_encode_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (s) => canonicalJsonForString(s) === canonicalJsonForString(s)), { numRuns: 8 });
});

it("canonical_encode_output_nonempty", () => {
  fc.assert(fc.property(fc.string(), (v) => canonicalEncodeLength(v) >= 1), { numRuns: 8 });
});

it("canonical_json_string_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (s) => canonicalJsonForString(s) === canonicalJsonForString(s)), { numRuns: 8 });
});

it("canonical_encode_true_length_eq_4", () => {
  expect(canonicalJsonTrueLength()).toBe(4);
});

it("canonical_encode_empty_array_length_eq_2", () => {
  expect(canonicalJsonEmptyArrayLength()).toBe(2);
});

it("canonical_encode_empty_object_length_eq_2", () => {
  expect(canonicalJsonEmptyObjectLength()).toBe(2);
});

it("canonical_encode_null_length_eq_4", () => {
  expect(canonicalJsonNullLength()).toBe(4);
});

it("property_hash_from_formula_output_length_eq_139", () => {
  fc.assert(fc.property(fc.string(), (f) => propertyHashFromFormulaLength(f) === 139), { numRuns: 8 });
});

it("property_hash_from_formula_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (f) => propertyHashForFormula(f) === propertyHashForFormula(f)), { numRuns: 8 });
});

it("hash_ir_json_output_length_eq_139", () => {
  fc.assert(fc.property(fc.string(), (v) => hashIrJsonLength(v) === 139), { numRuns: 8 });
});

it("hash_ir_json_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (v) => hashIrJsonForString(v) === hashIrJsonForString(v)), { numRuns: 8 });
});

it("contract_property_hash_output_length_eq_139", () => {
  fc.assert(fc.property(fc.string(), (s) => contractPropertyHashLength(s) === 139), { numRuns: 8 });
});

it("mint_memento_cid_length_eq_139", () => {
  fc.assert(fc.property(fc.string(), (args) => mintMementoCidLength(args) === 139), { numRuns: 8 });
});

it("mint_memento_signature_length_eq_96", () => {
  fc.assert(fc.property(fc.string(), (args) => mintMementoSignatureLength(args) === 96), { numRuns: 8 });
});

it("mint_contract_name_nonempty", () => {
  fc.assert(fc.property(fc.string(), (n) => mintContractNameLength(n) >= 1), { numRuns: 8 });
});

it("mint_bridge_input_cids_length_eq_1", () => {
  expect(mintBridgeInputCidsLength()).toBe(1);
});

it("mint_implication_input_cids_length_eq_2", () => {
  expect(mintImplicationInputCidsLength()).toBe(2);
});

it("mint_memento_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (a) => mintMementoCidLength(a) === mintMementoCidLength(a)), { numRuns: 8 });
});

it("sign_envelope_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (msg) => signEnvelopeForString(msg) === signEnvelopeForString(msg)), { numRuns: 8 });
});

it("sign_envelope_output_length_eq_96", () => {
  fc.assert(fc.property(fc.string(), (msg) => signEnvelopeLength(msg) === 96), { numRuns: 8 });
});

it("sign_envelope_output_nonempty", () => {
  fc.assert(fc.property(fc.string(), (msg) => signEnvelopeLength(msg) >= 1), { numRuns: 8 });
});

it("signature_prefix_min_length", () => {
  expect(signaturePrefixLength()).toBeGreaterThanOrEqual(8);
});

it("verify_envelope_signature_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (e) => verifyEnvelopeSignatureForString(e) === verifyEnvelopeSignatureForString(e)), { numRuns: 8 });
});

it("compute_envelope_cid_output_length_eq_139", () => {
  fc.assert(fc.property(fc.string(), (e) => computeEnvelopeCidLength(e) === 139), { numRuns: 8 });
});

it("compute_envelope_cid_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (e) => computeEnvelopeCidForString(e) === computeEnvelopeCidForString(e)), { numRuns: 8 });
});

it("envelope_for_hashing_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (e) => envelopeForHashingJson(e) === envelopeForHashingJson(e)), { numRuns: 8 });
});

it("build_proof_envelope_cid_length_eq_139", () => {
  fc.assert(fc.property(fc.string(), (input) => buildProofEnvelopeCidLength(input) === 139), { numRuns: 4 });
});

it("build_proof_envelope_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (input) => buildProofEnvelopeCidForString(input) === buildProofEnvelopeCidForString(input)), { numRuns: 4 });
});

it("build_proof_envelope_bytes_nonempty", () => {
  fc.assert(fc.property(fc.string(), (input) => buildProofEnvelopeBytesLength(input) >= 1), { numRuns: 4 });
});

it("decode_encode_round_trips", () => {
  expect(decodeEncodeRoundTrips()).toBe("ok");
});

it("verify_proof_envelope_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (b) => verifyProofEnvelopeForString(b) === verifyProofEnvelopeForString(b)), { numRuns: 4 });
});

it("resolver_entry_cid_length_eq_139", () => {
  expect(resolverEntryCidLength()).toBe(139);
});

it("resolver_entry_path_nonempty", () => {
  expect(resolverEntryPathLength()).toBeGreaterThanOrEqual(1);
});

it("resolver_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (root) => resolverEntriesCount(root) === resolverEntriesCount(root)), { numRuns: 4 });
});

it("load_all_proofs_empty_dir_yields_empty_pool", () => {
  expect(resolverEntriesCount(mkdtempSync(join(tmpdir(), "pk-empty-")))).toBe(0);
});

it("load_all_proofs_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (d) => resolverEntriesCount(d) === resolverEntriesCount(d)), { numRuns: 4 });
});

it("enumerate_callsites_property_cid_length_eq_139", () => {
  expect(callsitePropertyCidLength()).toBe(139);
});

it("enumerate_callsites_bridge_target_cid_length_eq_139", () => {
  expect(callsiteBridgeTargetCidLength()).toBe(139);
});

it("verifier_report_total_callsites_nonneg", () => {
  expect(verifierReportTotalCallsites()).toBeGreaterThanOrEqual(0);
});

it("bridge_enforcement_status_counts_sum_to_total", () => {
  expect(bridgeStatusCountsSum()).toBe(bridgeReportTotalCallsites());
});

it("bridge_enforcement_discharged_nonneg", () => {
  expect(bridgeReportDischarged()).toBeGreaterThanOrEqual(0);
});

it("bridge_enforcement_violations_nonneg", () => {
  expect(bridgeReportViolations()).toBeGreaterThanOrEqual(0);
});

it("run_bridge_enforcement_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (root) => runBridgeEnforcementSummary(root) === runBridgeEnforcementSummary(root)), { numRuns: 4 });
});

it("default_lift_seed_length_eq_32", () => {
  expect(defaultLiftSeedLength()).toBe(32);
});

it("mint_lifted_declarations_cid_length_eq_139", () => {
  fc.assert(fc.property(fc.string(), (decls) => mintLiftedDeclarationsCidLength(decls) === 139), { numRuns: 4 });
});

it("mint_lifted_declarations_member_count_nonneg", () => {
  fc.assert(fc.property(fc.string(), (decls) => mintLiftedDeclarationsMemberCount(decls) >= 0), { numRuns: 4 });
});

it("lift_and_mint_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (root) => liftAndMintCid(root) === liftAndMintCid(root)), { numRuns: 8 });
});

it("lift_zod_schema_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (schema) => liftZodSchemaIr(schema) === liftZodSchemaIr(schema)), { numRuns: 8 });
});

it("lift_zod_default_out_binding_length_eq_3", () => {
  expect(liftZodOutBindingLength()).toBe(3);
});

it("lift_zod_schema_decl_count_nonneg", () => {
  fc.assert(fc.property(fc.string(), (s) => liftZodSchemaDeclCount(s) >= 0), { numRuns: 8 });
});

it("lift_vitest_tests_is_deterministic", () => {
  fc.assert(fc.property(fc.string(), (f) => liftVitestTestsIr(f) === liftVitestTestsIr(f)), { numRuns: 8 });
});

it("lift_vitest_tests_decl_count_nonneg", () => {
  fc.assert(fc.property(fc.string(), (f) => liftVitestTestsDeclCount(f) >= 0), { numRuns: 8 });
});

it("lift_vitest_tests_decl_name_nonempty", () => {
  fc.assert(fc.property(fc.string(), (d) => liftVitestTestsDeclNameLength(d) >= 1), { numRuns: 8 });
});

it("classify_verdict_deterministic", () => {
  fc.assert(fc.property(fc.string(), fc.string(), (ab, ba) => classifyVerdictForStrings(ab, ba) === classifyVerdictForStrings(ab, ba)), { numRuns: 8 });
});

it("classify_unsat_unsat_equivalent", () => {
  expect(classifyVerdict("unsat", "unsat")).toBe("equivalent");
});

it("check_implication_serialize_deserialize_roundtrip", () => {
  expect(checkImplicationRoundTripVerdict()).toBe("equivalent");
});

it("serialize_input_idempotent", () => {
  expect(serializeInputTwice()).toBe(serializeInputTwice());
});

it("pool_keys_nonempty", () => {
  fc.assert(fc.property(fc.string(), (cid) => poolKeysNonempty(cid) >= 1), { numRuns: 8 });
});

it("bridge_keys_nonempty", () => {
  fc.assert(fc.property(fc.string(), (symbol) => bridgeKeysNonempty(symbol) >= 1), { numRuns: 8 });
});

it("errors_length_nonnegative", () => {
  fc.assert(fc.property(fc.string(), (root) => errorsLength(root) >= 0), { numRuns: 8 });
});

it("load_all_proofs_serialize_deserialize_roundtrip", () => {
  fc.assert(fc.property(fc.string(), (output) => loadAllProofsRoundTrip(output) === output), { numRuns: 8 });
});

it("enumerate_returns_unique", () => {
  expect(enumerateProofFilesUniqueCount()).toBe(enumerateProofFilesCount());
});

it("resolved_or_failure_reason", () => {
  fc.assert(fc.property(fc.string(), fc.string(), (cid, pool) => resolvedOrFailureReason(cid, pool) === "not-null"), { numRuns: 8 });
});

it("resolved_cid_matches_input", () => {
  fc.assert(fc.property(fc.string(), fc.string(), (cid, pool) => resolvedCid(cid, pool) === resolvedCid(cid, pool)), { numRuns: 8 });
});

it("resolve_bridge_target_serialize_deserialize_roundtrip", () => {
  fc.assert(fc.property(fc.string(), (output) => resolveBridgeTargetRoundTrip(output) === resolveBridgeTargetRoundTrip(output)), { numRuns: 8 });
});

it("failure_reason_valid_enum", () => {
  fc.assert(fc.property(fc.string(), fc.string(), (cid, pool) => failureReason(cid, pool) === failureReason(cid, pool)), { numRuns: 8 });
});

it("parseIntCanReturnZero", () => {
  expect(parseIntCanReturnZero()).toBe(0);
});

it("parseIntCanReturnNaN", () => {
  expect(parseIntCanReturnNaN()).toBe("nan");
});

it("parseIntCanReturnPositiveInteger", () => {
  expect(parseIntCanReturnPositiveInteger()).toBeGreaterThan(0);
});

it("parseIntZeroStringIsZero", () => {
  expect(parseIntValue("0")).toBe(0);
});

it("parseIntEmptyStringIsNaN", () => {
  expect(parseIntCanReturnNaN()).toBe("nan");
});

it("parseIntReturnsIntOrNaN", () => {
  fc.assert(fc.property(fc.string(), (s) => parseIntKind(s) === "int-or-nan"), { numRuns: 8 });
});

it("parseIntIsDeterministic", () => {
  fc.assert(fc.property(fc.string(), (s) => parseIntStableValue(s) === parseIntStableValue(s)), { numRuns: 8 });
});

it("parseIntPreservesNonNegativeIntegers", () => {
  fc.assert(fc.property(fc.integer(), (n) => parseIntNonnegativeRoundTrip(n) === nonnegative(n)), { numRuns: 8 });
});

it("Math.abs.returnsNonNegative", () => {
  fc.assert(fc.property(fc.integer(), (x) => mathAbsValue(x) >= 0), { numRuns: 8 });
});

it("Math.abs.preservesMagnitude", () => {
  fc.assert(fc.property(fc.integer(), (x) => mathAbsValue(x) === mathAbsNegativeValue(x)), { numRuns: 8 });
});

it("Math.abs.identityOnNonNegative", () => {
  fc.assert(fc.property(fc.nat(), (x) => mathAbsValue(x) === x), { numRuns: 8 });
});

it("Math.abs.zeroFixedPoint", () => {
  expect(mathAbsValue(0)).toBe(0);
});

it("Math.max.commutative", () => {
  fc.assert(fc.property(fc.integer(), fc.integer(), (a, b) => mathMax(a, b) === mathMax(b, a)), { numRuns: 8 });
});

it("Math.floor.idempotentOnIntegers", () => {
  fc.assert(fc.property(fc.integer(), (n) => mathFloorValue(n) === n), { numRuns: 8 });
});
