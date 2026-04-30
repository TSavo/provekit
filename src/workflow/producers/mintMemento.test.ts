/**
 * mintMemento Stage tests.
 */

import { describe, it, expect } from "vitest";
import {
  makeMintMementoStage,
  runMintMemento,
  MINT_MEMENTO_CAPABILITY,
} from "./mintMemento.js";
import { generateKeypair } from "../../producerKeys/index.js";

function fixedKeyPem(): string {
  const seed = Buffer.from(
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "hex",
  );
  const { privateKey } = generateKeypair({ seed });
  return privateKey.export({ type: "pkcs8", format: "pem" }).toString();
}

describe("mintMemento", () => {
  it("exposes the canonical capability name", () => {
    expect(MINT_MEMENTO_CAPABILITY).toBe("mint-memento");
  });

  it("mints a property memento from the loaded spec", async () => {
    const out = await runMintMemento({
      privateKeyPem: fixedKeyPem(),
      loaded: {
        kind: "property",
        spec: {
          bindingHash: "abcdef0123456789",
          propertyHash: "fedcba9876543210",
          verdict: "holds",
          producedBy: "test@v1",
          producedAt: "2026-01-01T00:00:00.000Z",
          inputCids: [],
          rawWitness: "{}",
        },
      },
    });
    expect(out.envelope.bindingHash).toBe("abcdef0123456789");
    expect(out.envelope.propertyHash).toBe("fedcba9876543210");
    expect(out.envelope.verdict).toBe("holds");
    expect(typeof out.envelope.cid).toBe("string");
    expect(out.publicKeyFingerprint).toMatch(/^[0-9a-f]{64}$/);
  });

  it("mints a bridge memento with default propertyHash derived from sourceSymbol", async () => {
    const out = await runMintMemento({
      privateKeyPem: fixedKeyPem(),
      loaded: {
        kind: "bridge",
        spec: {
          sourceSymbol: "parseInt",
          sourceLayer: "ts",
          targetContractCid: "bafy-target",
          targetLayer: "spec",
          producedAt: "2026-01-01T00:00:00.000Z",
        },
      },
    });
    expect(typeof out.envelope.cid).toBe("string");
    // The mint helper produces a bridge variant under the hood; we
    // don't assert on internal evidence shape here, only on the
    // top-level envelope contract.
    expect(out.envelope.bindingHash.length).toBe(16);
  });

  it("Stage shape: serializeInput uses the public-key fingerprint as cache key", () => {
    const stage = makeMintMementoStage();
    const serialized = stage.serializeInput({
      privateKeyPem: fixedKeyPem(),
      loaded: {
        kind: "property",
        spec: {
          bindingHash: "x",
          propertyHash: "y",
          producedBy: "z",
        },
      },
    });
    expect(serialized).toEqual({
      loaded: {
        kind: "property",
        spec: {
          bindingHash: "x",
          propertyHash: "y",
          producedBy: "z",
        },
      },
      publicKeyFingerprint: expect.stringMatching(/^[0-9a-f]{64}$/),
    });
  });
});
