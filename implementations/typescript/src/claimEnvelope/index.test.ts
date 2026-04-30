/**
 * Tests for the universal claim envelope module.
 *
 * Spec: protocol/specs/2026-04-29-universal-claim-envelope.md
 */

import { describe, it, expect } from "vitest";
import { generateKeyPairSync } from "node:crypto";
import {
  canonicalEncode,
  canonicalJsonString,
  computeEnvelopeCid,
  envelopeForHashing,
  signEnvelope,
  verifyEnvelopeSignature,
  validateEnvelope,
  VARIANT_SCHEMA_CIDS,
  VERDICTS,
  type ClaimEnvelope,
  type EvidenceVariant,
  type KeyResolver,
} from "./index.js";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SCHEMA_CID = VARIANT_SCHEMA_CIDS["z3-unsat"];

function makeLegacyEvidence(): EvidenceVariant {
  return {
    kind: "z3-unsat",
    schema: SCHEMA_CID,
    body: {
      smtLibInput: "(check-sat)",
      z3Verdict: "unsat",
      z3RunMs: 1,
    },
  };
}

function makeEnvelope(overrides: Partial<ClaimEnvelope> = {}): Omit<ClaimEnvelope, "cid"> {
  return {
    schemaVersion: "1",
    bindingHash: "a1b2c3d4e5f6a7b8",
    propertyHash: "b2c3d4e5f6a7b8c9",
    verdict: "holds",
    producedBy: "z3-symbolic@4.13.4",
    producedAt: "2026-04-29T12:00:00Z",
    inputCids: [],
    evidence: makeLegacyEvidence(),
    ...overrides,
  };
}

function makeFullEnvelope(overrides: Partial<ClaimEnvelope> = {}): ClaimEnvelope {
  const base = makeEnvelope(overrides);
  const cid = computeEnvelopeCid(base);
  return { ...base, cid } as ClaimEnvelope;
}

// ---------------------------------------------------------------------------
// 1. Canonical encoding
// ---------------------------------------------------------------------------

describe("canonicalEncode", () => {
  it("produces byte-identical output for equivalent objects", () => {
    const a = canonicalEncode({ z: 1, a: 2, m: "hello" });
    const b = canonicalEncode({ m: "hello", z: 1, a: 2 });
    expect(a.toString("utf-8")).toBe(b.toString("utf-8"));
  });

  it("sorts nested object keys recursively", () => {
    const result = canonicalJsonString({ b: { z: 1, a: 2 }, a: "x" });
    expect(result).toBe('{"a":"x","b":{"a":2,"z":1}}');
  });

  it("preserves array element order", () => {
    const result = canonicalJsonString([3, 1, 2]);
    expect(result).toBe("[3,1,2]");
  });

  it("handles null", () => {
    expect(canonicalJsonString(null)).toBe("null");
  });

  it("handles booleans", () => {
    expect(canonicalJsonString(true)).toBe("true");
    expect(canonicalJsonString(false)).toBe("false");
  });

  it("handles numbers", () => {
    expect(canonicalJsonString(42)).toBe("42");
    expect(canonicalJsonString(3.14)).toBe("3.14");
  });

  it("omits undefined keys", () => {
    const result = canonicalJsonString({ a: 1, b: undefined, c: 3 });
    expect(result).toBe('{"a":1,"c":3}');
  });

  it("produces no whitespace", () => {
    const result = canonicalJsonString({ a: 1, b: [1, 2] });
    expect(result).not.toMatch(/\s/);
  });

  it("produces UTF-8 bytes", () => {
    const bytes = canonicalEncode({ msg: "héllo" });
    expect(Buffer.isBuffer(bytes)).toBe(true);
    expect(bytes.toString("utf-8")).toContain("héllo");
  });
});

// ---------------------------------------------------------------------------
// 2. CID construction
// ---------------------------------------------------------------------------

describe("computeEnvelopeCid", () => {
  it("is deterministic across calls", () => {
    const env = makeEnvelope();
    const cid1 = computeEnvelopeCid(env);
    const cid2 = computeEnvelopeCid(env);
    expect(cid1).toBe(cid2);
  });

  it("returns 32 hex chars", () => {
    const cid = computeEnvelopeCid(makeEnvelope());
    expect(cid).toMatch(/^[0-9a-f]{32}$/);
  });

  it("changes when any wrapper field changes", () => {
    const base = makeEnvelope();
    const c1 = computeEnvelopeCid(base);
    const c2 = computeEnvelopeCid({ ...base, verdict: "violated" });
    const c3 = computeEnvelopeCid({ ...base, producedAt: "2026-04-29T13:00:00Z" });
    expect(c1).not.toBe(c2);
    expect(c1).not.toBe(c3);
    expect(c2).not.toBe(c3);
  });

  it("is order-independent for inputCids", () => {
    const cidA = "a".repeat(32);
    const cidB = "b".repeat(32);
    const e1 = computeEnvelopeCid(makeEnvelope({ inputCids: [cidA, cidB] }));
    const e2 = computeEnvelopeCid(makeEnvelope({ inputCids: [cidB, cidA] }));
    expect(e1).toBe(e2);
  });

  it("does not include the cid field itself in the hash", () => {
    const env = makeEnvelope();
    const cid1 = computeEnvelopeCid({ ...env, cid: undefined });
    const cid2 = computeEnvelopeCid({ ...env, cid: "deadbeefdeadbeefdeadbeefdeadbeef" });
    expect(cid1).toBe(cid2);
  });

  it("does not include producerSignature in the hash", () => {
    const env = makeEnvelope();
    const cid1 = computeEnvelopeCid({ ...env, producerSignature: undefined });
    const cid2 = computeEnvelopeCid({ ...env, producerSignature: "some-sig" });
    expect(cid1).toBe(cid2);
  });
});

// ---------------------------------------------------------------------------
// 3. Signing and verification
// ---------------------------------------------------------------------------

describe("signEnvelope / verifyEnvelopeSignature", () => {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");

  it("produces a non-empty base64 signature", () => {
    const sig = signEnvelope(makeEnvelope(), privateKey);
    expect(typeof sig).toBe("string");
    expect(sig.length).toBeGreaterThan(0);
    // Base64: only alphanumeric + / + = allowed
    expect(sig).toMatch(/^[A-Za-z0-9+/]+=*$/);
  });

  it("verifies a valid signature", () => {
    const env = makeEnvelope();
    const sig = signEnvelope(env, privateKey);
    const withSig = { ...env, producerSignature: sig };
    expect(verifyEnvelopeSignature(withSig, publicKey)).toBe(true);
  });

  it("rejects a forged signature", () => {
    const env = makeEnvelope();
    const forgedSig = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const withSig = { ...env, producerSignature: forgedSig };
    expect(verifyEnvelopeSignature(withSig, publicKey)).toBe(false);
  });

  it("rejects if producerSignature is absent", () => {
    const env = makeEnvelope();
    expect(verifyEnvelopeSignature(env, publicKey)).toBe(false);
  });

  it("rejects if a field is tampered after signing", () => {
    const env = makeEnvelope();
    const sig = signEnvelope(env, privateKey);
    const tampered = { ...env, verdict: "violated" as const, producerSignature: sig };
    expect(verifyEnvelopeSignature(tampered, publicKey)).toBe(false);
  });

  it("is consistent: same data → same verifiable signature", () => {
    const env = makeEnvelope();
    const sig1 = signEnvelope(env, privateKey);
    const sig2 = signEnvelope(env, privateKey);
    // ed25519 is deterministic given same key + data
    expect(sig1).toBe(sig2);
  });

  it("rejects with wrong public key", () => {
    const { publicKey: otherKey } = generateKeyPairSync("ed25519");
    const env = makeEnvelope();
    const sig = signEnvelope(env, privateKey);
    const withSig = { ...env, producerSignature: sig };
    expect(verifyEnvelopeSignature(withSig, otherKey)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// 4. Validation
// ---------------------------------------------------------------------------

describe("validateEnvelope", () => {
  it("accepts a well-formed envelope", () => {
    const env = makeFullEnvelope();
    const result = validateEnvelope(env);
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  it("rejects non-object input", () => {
    expect(validateEnvelope(null).valid).toBe(false);
    expect(validateEnvelope("string").valid).toBe(false);
    expect(validateEnvelope(42).valid).toBe(false);
  });

  it("rejects wrong schemaVersion", () => {
    const env = makeFullEnvelope({ schemaVersion: "2" as "1" });
    const result = validateEnvelope({ ...env, schemaVersion: "2" });
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("schemaVersion"))).toBe(true);
  });

  it("rejects bindingHash that is not 16 hex chars", () => {
    const env = { ...makeFullEnvelope(), bindingHash: "tooshort" };
    const result = validateEnvelope(env);
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("bindingHash"))).toBe(true);
  });

  it("rejects propertyHash that is not 16 hex chars", () => {
    const env = { ...makeFullEnvelope(), propertyHash: "ZZZ" };
    const result = validateEnvelope(env);
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("propertyHash"))).toBe(true);
  });

  it("rejects unknown verdict", () => {
    const raw = makeEnvelope();
    const cid = computeEnvelopeCid({ ...raw, verdict: "unknown" as any });
    const env = { ...raw, verdict: "unknown", cid };
    const result = validateEnvelope(env);
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("verdict"))).toBe(true);
  });

  it("rejects malformed producedBy", () => {
    const raw = makeEnvelope({ producedBy: "no-at-sign" });
    const cid = computeEnvelopeCid(raw);
    const result = validateEnvelope({ ...raw, cid });
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("producedBy"))).toBe(true);
  });

  it("accepts producedBy with colon in name (e.g. llm:claude-opus@4-7)", () => {
    const raw = makeEnvelope({ producedBy: "llm:claude-opus@4-7" });
    const cid = computeEnvelopeCid(raw);
    const result = validateEnvelope({ ...raw, cid });
    expect(result.errors.filter((e) => e.includes("producedBy"))).toHaveLength(0);
  });

  it("rejects mismatched CID", () => {
    const env = makeFullEnvelope();
    const tampered = { ...env, cid: "deadbeefdeadbeefdeadbeefdeadbeef" };
    const result = validateEnvelope(tampered);
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("CID integrity"))).toBe(true);
  });

  it("rejects inputCids with non-hex32 entries", () => {
    const raw = makeEnvelope({ inputCids: ["tooshort"] });
    const cid = computeEnvelopeCid(raw);
    const result = validateEnvelope({ ...raw, cid });
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("inputCids"))).toBe(true);
  });

  it("accepts inputCids with valid hex32 entries", () => {
    const raw = makeEnvelope({ inputCids: ["a".repeat(32), "b".repeat(32)] });
    const cid = computeEnvelopeCid(raw);
    const result = validateEnvelope({ ...raw, cid });
    expect(result.errors.filter((e) => e.includes("inputCids"))).toHaveLength(0);
  });

  it("warns about unresolved signature when no keyResolver", () => {
    const { privateKey } = generateKeyPairSync("ed25519");
    const env = makeEnvelope();
    const sig = signEnvelope(env, privateKey);
    const cid = computeEnvelopeCid(env);
    const full = { ...env, cid, producerSignature: sig };
    const result = validateEnvelope(full);
    expect(result.valid).toBe(true); // still valid
    expect(result.warnings.some((w) => w.includes("signature not verified"))).toBe(true);
  });

  it("errors on bad signature when keyResolver is present", () => {
    const { publicKey } = generateKeyPairSync("ed25519");
    const env = makeFullEnvelope();
    const withBadSig = { ...env, producerSignature: "AAAA" };
    const resolver: KeyResolver = () => publicKey;
    const result = validateEnvelope(withBadSig, { keyResolver: resolver });
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("verification failed"))).toBe(true);
  });

  it("accepts valid signature when keyResolver is present", () => {
    const { privateKey, publicKey } = generateKeyPairSync("ed25519");
    const env = makeEnvelope();
    const sig = signEnvelope(env, privateKey);
    const cid = computeEnvelopeCid(env);
    const full = { ...env, cid, producerSignature: sig };
    const resolver: KeyResolver = () => publicKey;
    const result = validateEnvelope(full, { keyResolver: resolver });
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  it("errors on missing signature in swarmMode", () => {
    const env = makeFullEnvelope();
    const result = validateEnvelope(env, { swarmMode: true });
    // warnings contain swarm notice
    expect(result.warnings.some((w) => w.includes("swarm mode recommends"))).toBe(true);
  });

  it("rejects envelope with missing required fields", () => {
    const result = validateEnvelope({ schemaVersion: "1" });
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(1);
  });
});

// ---------------------------------------------------------------------------
// 5. Evidence variant round-trips
// ---------------------------------------------------------------------------

describe("Evidence variant round-trips", () => {
  const variants: Array<{ label: string; evidence: EvidenceVariant; verdict: "holds" | "violated" }> = [
    {
      label: "z3-model",
      evidence: {
        kind: "z3-model",
        schema: VARIANT_SCHEMA_CIDS["z3-model"],
        body: {
          smtLibInput: "(assert (> x 0))",
          z3Verdict: "sat",
          model: "(define-fun x () Int 1)",
          counterexample: { x: 1 },
          z3RunMs: 12,
        },
      },
      verdict: "violated",
    },
    {
      label: "z3-unsat",
      evidence: {
        kind: "z3-unsat",
        schema: VARIANT_SCHEMA_CIDS["z3-unsat"],
        body: {
          smtLibInput: "(assert false)",
          z3Verdict: "unsat",
          z3RunMs: 5,
        },
      },
      verdict: "holds",
    },
    {
      label: "pattern-match",
      evidence: {
        kind: "pattern-match",
        schema: VARIANT_SCHEMA_CIDS["pattern-match"],
        body: {
          pattern: "CallExpression[callee.name=eval]",
          matchedNodes: ["node-1", "node-2"],
          matchedCaptures: { callee: "eval" },
        },
      },
      verdict: "violated",
    },
    {
      label: "type-check-pass",
      evidence: {
        kind: "type-check-pass",
        schema: VARIANT_SCHEMA_CIDS["type-check-pass"],
        body: {
          checker: "tsc",
          checkerVersion: "5.4.2",
          symbol: "myFn",
          resolvedType: "(x: number) => void",
          diagnosticsClean: true,
        },
      },
      verdict: "holds",
    },
    {
      label: "lint-pass",
      evidence: {
        kind: "lint-pass",
        schema: VARIANT_SCHEMA_CIDS["lint-pass"],
        body: {
          linter: "biome",
          linterVersion: "1.2.3",
          rulesetHash: "c".repeat(32),
          warnings: 0,
        },
      },
      verdict: "holds",
    },
    {
      label: "test-pass",
      evidence: {
        kind: "test-pass",
        schema: VARIANT_SCHEMA_CIDS["test-pass"],
        body: {
          runner: "vitest",
          runnerVersion: "4.1.4",
          testId: "myTest > case1",
          durationMs: 22,
        },
      },
      verdict: "holds",
    },
    {
      label: "test-fail",
      evidence: {
        kind: "test-fail",
        schema: VARIANT_SCHEMA_CIDS["test-fail"],
        body: {
          runner: "vitest",
          runnerVersion: "4.1.4",
          testId: "myTest > case2",
          durationMs: 30,
          failureDetail: "Expected 1 to equal 2",
        },
      },
      verdict: "violated",
    },
    {
      label: "llm-proposal",
      evidence: {
        kind: "llm-proposal",
        schema: VARIANT_SCHEMA_CIDS["llm-proposal"],
        body: {
          llm: "claude-opus",
          llmVersion: "4-7",
          promptCid: "d".repeat(32),
          proposedIrFormula: "(forall x (> x 0))",
          confidence: 0.92,
          rationale: "looks correct",
        },
      },
      verdict: "holds",
    },
    {
      label: "mutation-witness",
      evidence: {
        kind: "mutation-witness",
        schema: VARIANT_SCHEMA_CIDS["mutation-witness"],
        body: {
          testCid: "e".repeat(32),
          mutationCid: "f".repeat(32),
          failsOnOriginal: false,
          passesOnFixed: true,
        },
      },
      verdict: "holds",
    },
    {
      label: "workflow-run",
      evidence: {
        kind: "workflow-run",
        schema: VARIANT_SCHEMA_CIDS["workflow-run"],
        body: {
          workflowName: "bug-fix",
          workflowCid: "0".repeat(32),
          inputCanonicalForm: { issue: 42 },
          output: { status: "done" },
        },
      },
      verdict: "holds",
    },
  ];

  for (const { label, evidence, verdict } of variants) {
    it(`round-trips ${label} variant`, () => {
      const raw = makeEnvelope({ evidence, verdict });
      const cid = computeEnvelopeCid(raw);
      const full = { ...raw, cid };
      const result = validateEnvelope(full);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it(`${label}: CID is deterministic`, () => {
      const raw = makeEnvelope({ evidence, verdict });
      const cid1 = computeEnvelopeCid(raw);
      const cid2 = computeEnvelopeCid(raw);
      expect(cid1).toBe(cid2);
    });
  }
});

// ---------------------------------------------------------------------------
// 6. Edge cases
// ---------------------------------------------------------------------------

describe("Edge cases", () => {
  it("VERDICTS set contains all five values", () => {
    expect(VERDICTS.has("holds")).toBe(true);
    expect(VERDICTS.has("violated")).toBe(true);
    expect(VERDICTS.has("decayed")).toBe(true);
    expect(VERDICTS.has("undecidable")).toBe(true);
    expect(VERDICTS.has("error")).toBe(true);
  });

  it("envelopeForHashing strips cid and producerSignature keys", () => {
    const env = makeFullEnvelope();
    const forHash = envelopeForHashing({ ...env, producerSignature: "sig" });
    expect("cid" in forHash).toBe(false);
    expect("producerSignature" in forHash).toBe(false);
  });

  it("envelopeForHashing sorts inputCids lexicographically", () => {
    const env = makeEnvelope({ inputCids: ["zzz" + "0".repeat(29), "aaa" + "0".repeat(29)] });
    const forHash = envelopeForHashing(env);
    const cids = forHash.inputCids as string[];
    expect(cids[0]).toBe("aaa" + "0".repeat(29));
    expect(cids[1]).toBe("zzz" + "0".repeat(29));
  });

  it("changing evidence body changes the CID", () => {
    const ev1 = makeLegacyEvidence();
    const ev2 = {
      ...ev1,
      body: { smtLibInput: "(check-sat)\n", z3Verdict: "unsat" as const, z3RunMs: 2 },
    } as EvidenceVariant;
    const c1 = computeEnvelopeCid(makeEnvelope({ evidence: ev1 }));
    const c2 = computeEnvelopeCid(makeEnvelope({ evidence: ev2 }));
    expect(c1).not.toBe(c2);
  });

  it("canonicalEncode throws for NaN", () => {
    expect(() => canonicalJsonString(NaN)).toThrow();
  });

  it("canonicalEncode throws for Infinity", () => {
    expect(() => canonicalJsonString(Infinity)).toThrow();
  });
});
