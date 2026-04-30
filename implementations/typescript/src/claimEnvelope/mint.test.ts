import { describe, it, expect } from "vitest";
import { generateKeypair } from "../producerKeys/index.js";
import {
  mintMemento,
  mintBridge,
  mintLegacyWitness,
  mintProperty,
  mintAndVerifyMemento,
} from "./mint.js";
import { verifyEnvelopeSignature } from "./sign.js";
import { VARIANT_SCHEMA_CIDS } from "./variants/index.js";
import type { LegacyWitnessEvidence, PropertyEvidence } from "./types.js";
import type { IrFormula } from "../ir/formulas.js";

const SEED = Buffer.from("mint-test-seed-32-bytes-padding!").subarray(0, 32);

function makeLegacyEvidence(rawWitness = "{}", producerId = "test@1"): LegacyWitnessEvidence {
  return {
    kind: "legacy-witness",
    schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
    body: { rawWitness, legacyProducerId: producerId },
  };
}

describe("mintMemento", () => {
  it("produces a signed, content-addressed envelope", () => {
    const kp = generateKeypair({ seed: SEED });
    const memento = mintMemento({
      bindingHash: "0123456789abcdef",
      propertyHash: "fedcba9876543210",
      verdict: "holds",
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      inputCids: [],
      evidence: makeLegacyEvidence(),
      privateKey: kp.privateKey,
    });
    expect(memento.cid).toMatch(/^[0-9a-f]{32}$/);
    expect(memento.producerSignature).toBeTruthy();
    expect(verifyEnvelopeSignature(memento, kp.publicKey)).toBe(true);
  });

  it("sorts inputCids deterministically", () => {
    const kp = generateKeypair({ seed: SEED });
    const a = mintMemento({
      bindingHash: "0000000000000001",
      propertyHash: "0000000000000002",
      verdict: "holds",
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      inputCids: ["zzz", "aaa", "mmm"],
      evidence: makeLegacyEvidence(),
      privateKey: kp.privateKey,
    });
    const b = mintMemento({
      bindingHash: "0000000000000001",
      propertyHash: "0000000000000002",
      verdict: "holds",
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      inputCids: ["aaa", "mmm", "zzz"],
      evidence: makeLegacyEvidence(),
      privateKey: kp.privateKey,
    });
    expect(a.cid).toBe(b.cid);
    expect(a.inputCids).toEqual(["aaa", "mmm", "zzz"]);
  });

  it("defaults producedAt to now", () => {
    const kp = generateKeypair({ seed: SEED });
    const before = Date.now();
    const memento = mintMemento({
      bindingHash: "0000000000000001",
      propertyHash: "0000000000000002",
      verdict: "holds",
      producedBy: "test@1",
      inputCids: [],
      evidence: makeLegacyEvidence(),
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
    const targetCid = "abcdef0123456789abcdef0123456789";
    const bridge = mintBridge({
      bindingHash: "1111111111111111",
      propertyHash: "2222222222222222",
      producedBy: "ts-kit@1.0",
      producedAt: new Date(0).toISOString(),
      privateKey: kp.privateKey,
      sourceSymbol: "global.parseInt",
      sourceLayer: "TS-kit@1.0",
      targetContractCid: targetCid,
      targetLayer: "V8@12.4",
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
      bindingHash: "1111111111111111",
      propertyHash: "2222222222222222",
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      privateKey: kp.privateKey,
      sourceSymbol: "x",
      sourceLayer: "L1",
      targetContractCid: "deadbeef".repeat(4),
      targetLayer: "L2",
      notes: "verified empirically",
    });
    if (bridge.evidence.kind === "bridge") {
      expect(bridge.evidence.body.notes).toBe("verified empirically");
    }
  });

  it("omits notes field when not provided", () => {
    const kp = generateKeypair({ seed: SEED });
    const bridge = mintBridge({
      bindingHash: "1111111111111111",
      propertyHash: "2222222222222222",
      producedBy: "test@1",
      producedAt: new Date(0).toISOString(),
      privateKey: kp.privateKey,
      sourceSymbol: "x",
      sourceLayer: "L1",
      targetContractCid: "deadbeef".repeat(4),
      targetLayer: "L2",
    });
    if (bridge.evidence.kind === "bridge") {
      expect("notes" in bridge.evidence.body).toBe(false);
    }
  });
});

describe("mintLegacyWitness", () => {
  it("wraps a raw witness string in the legacy variant", () => {
    const kp = generateKeypair({ seed: SEED });
    const memento = mintLegacyWitness({
      bindingHash: "3333333333333333",
      propertyHash: "4444444444444444",
      producedBy: "z3-symbolic@4.13",
      producedAt: new Date(0).toISOString(),
      privateKey: kp.privateKey,
      rawWitness: '{"verdict":"holds","model":{}}',
    });
    expect(memento.evidence.kind).toBe("legacy-witness");
    if (memento.evidence.kind === "legacy-witness") {
      expect(memento.evidence.body.rawWitness).toBe(
        '{"verdict":"holds","model":{}}',
      );
      expect(memento.evidence.body.legacyProducerId).toBe("z3-symbolic@4.13");
    }
    expect(verifyEnvelopeSignature(memento, kp.publicKey)).toBe(true);
  });
});

describe("mintProperty", () => {
  it("embeds the IR formula directly in evidence.body, not stringified", () => {
    const kp = generateKeypair({ seed: SEED });
    const stringSort = { kind: "primitive" as const, name: "String" };
    const irFormula: IrFormula = {
      kind: "forall",
      sort: stringSort,
      predicate: {
        kind: "lambda",
        varName: "s",
        sort: stringSort,
        body: {
          kind: "atomic",
          predicate: "nonempty",
          args: [{ kind: "var", name: "s", sort: stringSort }],
        },
      },
    };
    const memento = mintProperty({
      bindingHash: "aaaa1111aaaa1111",
      propertyHash: "bbbb2222bbbb2222",
      producedBy: "cpp-kit@1",
      privateKey: kp.privateKey,
      irFormula,
      scope: { kind: "function", name: "parseInt" },
      irKitVersion: "cpp-kit@1.0",
    });
    expect(memento.evidence.kind).toBe("property");
    const ev = memento.evidence as PropertyEvidence;
    expect(ev.body.irKitVersion).toBe("cpp-kit@1.0");
    expect(ev.body.irFormula).toEqual(irFormula);
    expect(ev.body.scope).toEqual({ kind: "function", name: "parseInt" });
    expect(verifyEnvelopeSignature(memento, kp.publicKey)).toBe(true);
  });
});

describe("mintAndVerifyMemento", () => {
  it("returns the memento on successful verification", () => {
    const kp = generateKeypair({ seed: SEED });
    const memento = mintAndVerifyMemento(
      {
        bindingHash: "5555555555555555",
        propertyHash: "6666666666666666",
        verdict: "holds",
        producedBy: "test@1",
        producedAt: new Date(0).toISOString(),
        inputCids: [],
        evidence: makeLegacyEvidence(),
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
          bindingHash: "5555555555555555",
          propertyHash: "6666666666666666",
          verdict: "holds",
          producedBy: "test@1",
          producedAt: new Date(0).toISOString(),
          inputCids: [],
          evidence: makeLegacyEvidence(),
          privateKey: kp.privateKey,
        },
        otherKp.publicKey, // wrong public key
      ),
    ).toThrow(/signature verification failed/);
  });
});
