// SPDX-License-Identifier: Apache-2.0
//
// provekit-self-contracts (zig): Side B substrate library.
//
// Native Zig implementation of the substrate primitives required to mint
// claim envelopes (JCS-canonical layered mementos) and proof envelopes
// (deterministic-CBOR signed catalogs). No shell-out to peer kits.
//
// Public API:
//   const sc = @import("provekit-self-contracts");
//   sc.jcs.encode(alloc, value)
//   sc.hash.blake3_512Of(alloc, bytes)
//   sc.cbor.encodeUint(alloc, &buf, n)
//   sc.signing.signString(alloc, seed, msg)
//   sc.signing.verifyString(pubkey, sig, msg)
//   sc.proof_envelope.build(alloc, input)
//   sc.claim_envelope.mintContract(alloc, args)
//   sc.foundation.SEED  // [0x42; 32]

const std = @import("std");

pub const jcs = @import("jcs.zig");
pub const hash = @import("hash.zig");
pub const cbor = @import("cbor.zig");
pub const signing = @import("signing.zig");
pub const foundation = @import("foundation.zig");
pub const proof_envelope = @import("proof_envelope.zig");
pub const claim_envelope = @import("claim_envelope.zig");

// Pull in tests from every sibling file so `zig build test` discovers them.
test {
    _ = @import("jcs.zig");
    _ = @import("hash.zig");
    _ = @import("cbor.zig");
    _ = @import("signing.zig");
    _ = @import("foundation.zig");
    _ = @import("proof_envelope.zig");
    _ = @import("claim_envelope.zig");
    _ = @import("byte_equivalence_test.zig");
    _ = @import("cicp_conformance_test.zig");
}
