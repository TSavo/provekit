// Capture deterministic .proof bytes for a fixed input. Runs as a
// regular vitest test so its output prints the hex; the C++
// conformance test pins to the same bytes.

import { describe, it } from "vitest";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { buildProofEnvelope } from "./index.js";
import { generateKeypair } from "../producerKeys/index.js";

describe("proof-envelope cross-lang fixture", () => {
  it("emits deterministic bytes for the canary input", () => {
    const seed = Buffer.alloc(32, 0x42);
    const { privateKey, publicKey } = generateKeypair({ seed });
    const built = buildProofEnvelope({
      name: "test",
      version: "1.0.0",
      members: new Map(),
      signerCid: "sha256:000000000000abcd",
      signerPrivateKey: privateKey,
      declaredAt: "2026-04-30T12:00:00.000Z",
    });
    const out = {
      proofBytesHex: Buffer.from(built.bytes).toString("hex"),
      proofByteLength: built.bytes.length,
      filenameCid: built.cid,
      publicKeySpkiHex: publicKey.export({ type: "spki", format: "der" }).toString("hex"),
    };
    writeFileSync(
      resolve(process.cwd(), "scripts/cross-lang-equivalence/proof-envelope.fixture.json"),
      JSON.stringify(out, null, 2),
    );
  });
});
