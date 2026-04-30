// Capture deterministic .proof bytes for a fixed input. The hex output
// becomes the C++ conformance test's expected value.
//
// Inputs frozen so both impls have the same target:
//   name        = "test"
//   version     = "1.0.0"
//   declaredAt  = "2026-04-30T12:00:00.000Z"
//   members     = {} (empty — simplest fixture)
//   signerCid   = "sha256:000000000000abcd"
//   signerSeed  = 32 bytes of 0x42

import { buildProofEnvelope } from "../../implementations/typescript/src/proofEnvelope/index.js";
import { generateKeypair } from "../../implementations/typescript/src/producerKeys/index.js";

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

console.log(JSON.stringify({
  inputs: {
    name: "test",
    version: "1.0.0",
    declaredAt: "2026-04-30T12:00:00.000Z",
    members: "{}",
    signerCid: "sha256:000000000000abcd",
    signerSeed: "0x42 * 32",
  },
  publicKeySpkiHex: publicKey.export({ type: "spki", format: "der" }).toString("hex"),
  proofBytesHex: Buffer.from(built.bytes).toString("hex"),
  proofByteLength: built.bytes.length,
  filenameCid: built.cid,
}, null, 2));
