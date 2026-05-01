import { describe, it, expect } from "vitest";
import { generateKeypair } from "../producerKeys/index.js";
import {
  mintMemento,
  mintBridge,
  mintContract,
  mintImplication,
  mintAndVerifyMemento,
} from "./mint.js";
import { verifyEnvelopeSignature } from "./sign.js";
import { VARIANT_SCHEMA_CIDS } from "./variants/index.js";
import type {
  ContractEvidence,
  ImplicationEvidence,
  Z3UnsatEvidence,
} from "./types.js";
import type { IrFormula } from "../ir/formulas.js";

const SEED = Buffer.from("mint-test-seed-32-bytes-padding!").subarray(0, 32);

const SELF_ID_HASH = /^[a-z0-9]+-[0-9]+:[0-9a-f]+$/;
const BLAKE3_512_CID = /^blake3-512:[0-9a-f]{128}$/;

/**
 * Build a placeholder self-identifying hash so test fixtures pass the
 * v1.1.0 regex without needing a real BLAKE3 computation.
 */
function fakeCid(suffix: string): string {
  return `blake3-512:${suffix.padStart(128, "0")}`;
}

function makeUnsatEvidence(): Z3UnsatEvidence {
  return {
    kind: "z3-unsat",
    schema: VARIANT_SCHEMA_CIDS["z3-unsat"]!,
    body: {
      smtLibInput: "(check-sat)\n",
      z3Verdict: "unsat",
      z3RunMs: 1,
    },
  };
}

describe("mintMemento", () => {
  it("produces a signed, content-addressed envelope", () => {
    const kp = generateKeypair({ seed: SEED });
    const memento = mintMemento({
      bindingHash: fakeCid("0123456789abcdef"),
      propertyHash: fakeCid("fedcba9876543210"),
      verdict: "holds",
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      inputCids: [],
      evidence: makeUnsatEvidence(),
      privateKey: kp.privateKey,
    });
    expect(memento.cid).toMatch(BLAKE3_512_CID);
    expect(memento.producerSignature).toBeTruthy();
    expect(verifyEnvelopeSignature(memento, kp.publicKey)).toBe(true);
  });

  it("sorts inputCids deterministically", () => {
    const kp = generateKeypair({ seed: SEED });
    const cidA = fakeCid("a".repeat(64));
    const cidB = fakeCid("b".repeat(64));
    const cidC = fakeCid("c".repeat(64));
    const a = mintMemento({
      bindingHash: fakeCid("01"),
      propertyHash: fakeCid("02"),
      verdict: "holds",
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      inputCids: [cidC, cidA, cidB],
      evidence: makeUnsatEvidence(),
      privateKey: kp.privateKey,
    });
    const b = mintMemento({
      bindingHash: fakeCid("01"),
      propertyHash: fakeCid("02"),
      verdict: "holds",
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      inputCids: [cidA, cidB, cidC],
      evidence: makeUnsatEvidence(),
      privateKey: kp.privateKey,
    });
    expect(a.cid).toBe(b.cid);
    expect(a.inputCids).toEqual([cidA, cidB, cidC]);
  });

  it("defaults producedAt to now", () => {
    const kp = generateKeypair({ seed: SEED });
    const before = Date.now();
    const memento = mintMemento({
      bindingHash: fakeCid("01"),
      propertyHash: fakeCid("02"),
      verdict: "holds",
      producedBy: "test@1",
      inputCids: [],
      evidence: makeUnsatEvidence(),
      privateKey: kp.privateKey,
    });
    const after = Date.now();
    const ts = Date.parse(memento.producedAt);
    expect(ts).toBeGreaterThanOrEqual(before);
    expect(ts).toBeLessThanOrEqual(after);
  });
});

describe("mintBridge", () => {
  it("produces a bridge memento with targetContractCid in inputCids", () => {
    const kp = generateKeypair({ seed: SEED });
    const targetCid = fakeCid("abcdef0123456789abcdef0123456789");
    const bridge = mintBridge({
      producedBy: "ts-kit@1.0",
      producedAt: new Date(0).toISOString(),
      privateKey: kp.privateKey,
      sourceSymbol: "global.parseInt",
      sourceLayer: "TS-kit@1.0",
      targetContractCid: targetCid,
      targetLayer: "V8@12.4",
      irArgSorts: ["String"],
      irReturnSort: "Int",
    });
    expect(bridge.evidence.kind).toBe("bridge");
    expect(bridge.inputCids).toEqual([targetCid]);
    expect(bridge.verdict).toBe("holds");
    if (bridge.evidence.kind === "bridge") {
      expect(bridge.evidence.body.sourceSymbol).toBe("global.parseInt");
      expect(bridge.evidence.body.targetContractCid).toBe(targetCid);
      expect(bridge.evidence.body.targetLayer).toBe("V8@12.4");
    }
    expect(verifyEnvelopeSignature(bridge, kp.publicKey)).toBe(true);
  });

  it("optionally includes notes", () => {
    const kp = generateKeypair({ seed: SEED });
    const bridge = mintBridge({
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      privateKey: kp.privateKey,
      sourceSymbol: "x",
      sourceLayer: "L1",
      targetContractCid: fakeCid("deadbeef".repeat(16)),
      targetLayer: "L2",
      irArgSorts: ["String"],
      irReturnSort: "Int",
      notes: "verified empirically",
    });
    if (bridge.evidence.kind === "bridge") {
      expect(bridge.evidence.body.notes).toBe("verified empirically");
    }
  });

  it("omits notes field when not provided", () => {
    const kp = generateKeypair({ seed: SEED });
    const bridge = mintBridge({
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      privateKey: kp.privateKey,
      sourceSymbol: "x",
      sourceLayer: "L1",
      targetContractCid: fakeCid("deadbeef".repeat(16)),
      targetLayer: "L2",
      irArgSorts: ["String"],
      irReturnSort: "Int",
    });
    if (bridge.evidence.kind === "bridge") {
      expect("notes" in bridge.evidence.body).toBe(false);
    }
  });
});

describe("mintContract", () => {
  it("embeds the IR formula directly in evidence.body.pre, not stringified", () => {
    const kp = generateKeypair({ seed: SEED });
    const stringSort = { kind: "primitive" as const, name: "String" };
    const irFormula: IrFormula = {
      kind: "forall",
      name: "s",
      sort: stringSort,
      body: {
        kind: "atomic",
        name: "nonempty",
        args: [{ kind: "var", name: "s" }],
      },
    };
    const memento = mintContract({
      producedBy: "cpp-kit@1",
      privateKey: kp.privateKey,
      contractName: "parseInt",
      pre: irFormula,
      authoring: { producerKind: "kit-author", author: "cpp-kit@1" },
    });
    expect(memento.evidence.kind).toBe("contract");
    const ev = memento.evidence as ContractEvidence;
    expect(ev.body.contractName).toBe("parseInt");
    expect(ev.body.outBinding).toBe("out");
    expect(ev.body.pre).toEqual(irFormula);
    expect(ev.body.preHash).toMatch(BLAKE3_512_CID);
    expect(memento.bindingHash).toMatch(SELF_ID_HASH);
    expect(memento.propertyHash).toMatch(SELF_ID_HASH);
    expect(verifyEnvelopeSignature(memento, kp.publicKey)).toBe(true);
  });

  it("rejects an empty contract (no pre/post/inv)", () => {
    const kp = generateKeypair({ seed: SEED });
    expect(() =>
      mintContract({
        producedBy: "cpp-kit@1",
        privateKey: kp.privateKey,
        contractName: "empty",
        authoring: { producerKind: "kit-author", author: "cpp-kit@1" },
      }),
    ).toThrow(/at least one of pre\/post\/inv/);
  });
});

describe("mintImplication", () => {
  it("derives bindingHash, propertyHash, and inputCids from antecedent/consequent", () => {
    const kp = generateKeypair({ seed: SEED });
    const aCid = fakeCid("1".repeat(64));
    const cCid = fakeCid("2".repeat(64));
    const aHash = fakeCid("a".repeat(64));
    const bHash = fakeCid("b".repeat(64));
    const memento = mintImplication({
      producedBy: "z3@4.13.4",
      privateKey: kp.privateKey,
      antecedentHash: aHash,
      consequentHash: bHash,
      antecedentCid: cCid, // intentionally swapped to verify lex-sort
      consequentCid: aCid,
      antecedentSlot: "post",
      consequentSlot: "pre",
      prover: "z3@4.13.4",
      proverRunMs: 47,
    });
    expect(memento.evidence.kind).toBe("implication");
    expect(memento.inputCids).toEqual([aCid, cCid]);
    const ev = memento.evidence as ImplicationEvidence;
    expect(ev.body.antecedentHash).toBe(aHash);
    expect(verifyEnvelopeSignature(memento, kp.publicKey)).toBe(true);
  });
});

describe("mintAndVerifyMemento", () => {
  it("returns the memento on successful verification", () => {
    const kp = generateKeypair({ seed: SEED });
    const memento = mintAndVerifyMemento(
      {
        bindingHash: fakeCid("55"),
        propertyHash: fakeCid("66"),
        verdict: "holds",
        producedBy: "test@1",
        producedAt: new Date(0).toISOString(),
        inputCids: [],
        evidence: makeUnsatEvidence(),
        privateKey: kp.privateKey,
      },
      kp.publicKey,
    );
    expect(memento.producerSignature).toBeTruthy();
  });

  it("throws on verification failure", () => {
    const kp = generateKeypair({ seed: SEED });
    const otherSeed = Buffer.from("other-key-seed-padded-32-bytes!!").subarray(0, 32);
    const otherKp = generateKeypair({ seed: otherSeed });
    expect(() =>
      mintAndVerifyMemento(
        {
          bindingHash: fakeCid("55"),
          propertyHash: fakeCid("66"),
          verdict: "holds",
          producedBy: "test@1",
          producedAt: new Date(0).toISOString(),
          inputCids: [],
          evidence: makeUnsatEvidence(),
          privateKey: kp.privateKey,
        },
        otherKp.publicKey,
      ),
    ).toThrow(/signature verification failed/);
  });
});
