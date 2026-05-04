// SPDX-License-Identifier: Apache-2.0
//
// Byte-equivalence pins for the substrate library. These tests demonstrate
// that the Zig substrate produces byte-identical output to the Rust + Go
// reference kits when given the same inputs.
//
// Anchor 1 — Foundation v0 pubkey (signing.zig):
//   `ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=`
//   Pinned in every .provekit/self-contracts-attestations/*.json file
//   (the `signer` field). Rust derives this via ed25519-dalek; Zig via
//   std.crypto.sign.Ed25519. Both must agree.
//
// Anchor 2 — Empty contractSetCid (claim_envelope.zig):
//   `blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229`
//   Pinned in .provekit/self-contracts-attestations/zig.json (and several
//   other empty-set kits). It is BLAKE3-512(JCS([]).bytes); JCS-of-empty-
//   array is exactly the two bytes "[]". A Zig kit that disagrees here
//   has either a JCS bug or a BLAKE3 bug.
//
// Anchor 3 — Full self-contracts attestation message round-trip:
//   Rebuild the exact JCS message body that produced
//   .provekit/self-contracts-attestations/zig.json, sign it under the
//   foundation v0 seed, and verify the signature equals the one stored
//   in that file. Demonstrates JCS + Ed25519 byte-equivalence end-to-end
//   against the live attestation infrastructure.
//
// Anchor 4 — Cross-kit JCS+BLAKE3 of IR Decls is already pinned in the
//   sibling provekit-ir package (`cross_kit_bridges.zig` —
//   `PINNED_COUNTERPART_CIDS`, `PINNED_BRIDGES_ARRAY_CID`). Running
//   `zig build test` in that package proves byte-equivalence against the
//   rust/python/go/ts kits' pinned tables.

const std = @import("std");
const sc = @import("root.zig");

const Value = sc.jcs.Value;
const ObjectBuilder = sc.jcs.ObjectBuilder;

// ---- Anchor 1: foundation pubkey ------------------------------------------

test "anchor 1: foundation v0 pubkey matches the cross-kit pin" {
    const alloc = std.testing.allocator;
    const pk = try sc.signing.pubkeyString(alloc, sc.foundation.SEED);
    defer alloc.free(pk);
    try std.testing.expectEqualStrings(
        "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
        pk,
    );
}

// ---- Anchor 2: empty contractSetCid ---------------------------------------

test "anchor 2: empty contract set CID matches zig.json attestation pin" {
    const alloc = std.testing.allocator;
    const empty: [][]const u8 = &.{};
    const cid = try sc.claim_envelope.computeContractSetCid(alloc, @constCast(empty));
    defer alloc.free(cid);
    try std.testing.expectEqualStrings(
        "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229",
        cid,
    );
}

test "anchor 2 corollary: JCS([]) is the literal two bytes \"[]\"" {
    const alloc = std.testing.allocator;
    var ab = sc.jcs.ArrayBuilder.init(alloc);
    const v = try ab.finish();
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    const enc = try sc.jcs.encode(alloc, v);
    defer alloc.free(enc);
    try std.testing.expectEqualStrings("[]", enc);
}

// ---- Anchor 3: full attestation message round-trip ------------------------
//
// The attestation file at .provekit/self-contracts-attestations/zig.json
// was signed by tools/foundation-keygen over the JCS-canonical bytes of:
//   {
//     "schemaVersion": "1",
//     "kind": "self-contracts-attestation",
//     "lang": "zig",
//     "cid": "",
//     "contractSetCid": "blake3-512:d53d18c2...290f3229",
//     "declaredAt": "2026-05-03T18:00:00Z",
//     "signer": "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI="
//   }
// (signature field NOT included in the signed body — the rust signer
// excludes it; see tools/foundation-keygen/src/lib.rs::build_self_contracts_message).
// This test rebuilds that JCS bytes natively in Zig, signs it, and asserts
// the signature equals the one committed in zig.json. Cross-kit byte
// agreement is empirically demonstrated.

const ZIG_JSON_SIGNATURE: []const u8 =
    "ed25519:dJC/GluB7pdJ83gw5v8ky1liE8nGAgxLIo7pneMsZFZhdmkxHayRnHtUli4+iiIOb7462L87AChMFWVpIJgeAw==";

fn buildAttestationMessage(alloc: std.mem.Allocator) !*Value {
    var ob = ObjectBuilder.init(alloc);
    try ob.add("schemaVersion", try Value.newString(alloc, "1"));
    try ob.add("kind", try Value.newString(alloc, "self-contracts-attestation"));
    try ob.add("lang", try Value.newString(alloc, "zig"));
    try ob.add("cid", try Value.newString(alloc, ""));
    try ob.add("contractSetCid", try Value.newString(alloc, "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229"));
    try ob.add("declaredAt", try Value.newString(alloc, "2026-05-03T18:00:00Z"));
    try ob.add("signer", try Value.newString(alloc, "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI="));
    return ob.finish();
}

test "anchor 3: zig attestation signature byte-matches zig.json" {
    const alloc = std.testing.allocator;
    const msg_v = try buildAttestationMessage(alloc);
    defer {
        msg_v.deinit(alloc);
        alloc.destroy(msg_v);
    }
    const jcs_bytes = try sc.jcs.encode(alloc, msg_v);
    defer alloc.free(jcs_bytes);

    const sig = try sc.signing.signString(alloc, sc.foundation.SEED, jcs_bytes);
    defer alloc.free(sig);

    try std.testing.expectEqualStrings(ZIG_JSON_SIGNATURE, sig);
}

test "anchor 3 corollary: signature verifies against the foundation pubkey" {
    const alloc = std.testing.allocator;
    const msg_v = try buildAttestationMessage(alloc);
    defer {
        msg_v.deinit(alloc);
        alloc.destroy(msg_v);
    }
    const jcs_bytes = try sc.jcs.encode(alloc, msg_v);
    defer alloc.free(jcs_bytes);

    const pk = try sc.signing.pubkeyString(alloc, sc.foundation.SEED);
    defer alloc.free(pk);
    try std.testing.expect(sc.signing.verifyString(pk, ZIG_JSON_SIGNATURE, jcs_bytes));
}

// ---- Cross-kit attestation: Rust kit signature byte-matches too -----------

test "rust attestation signature byte-matches rust.json" {
    // Same shape, different `lang` and `cid` and `contractSetCid` — proves
    // the JCS encoder + Ed25519 signer agree across non-empty-bundle kits.
    const alloc = std.testing.allocator;
    var ob = ObjectBuilder.init(alloc);
    try ob.add("schemaVersion", try Value.newString(alloc, "1"));
    try ob.add("kind", try Value.newString(alloc, "self-contracts-attestation"));
    try ob.add("lang", try Value.newString(alloc, "rust"));
    try ob.add("cid", try Value.newString(alloc, "blake3-512:60df6322388ff7d9ccd1b9ee9d6457fdfe89a51b3d2d73a34daa0131f65c80543331832eb88920fe514ebb799bc655808f152d36274542ac9879c862a33f3a92"));
    try ob.add("contractSetCid", try Value.newString(alloc, "blake3-512:8f4bcc3c3e748ae303f8c8da80245f291e803eb2d241224c75c7ac470631e4dee7ff2e0ff59af571db3d506485c115acaddfd91e4e4315eb04ee37c035ddbc69"));
    try ob.add("declaredAt", try Value.newString(alloc, "2026-05-03T18:00:00Z"));
    try ob.add("signer", try Value.newString(alloc, "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI="));
    const msg_v = try ob.finish();
    defer {
        msg_v.deinit(alloc);
        alloc.destroy(msg_v);
    }
    const jcs_bytes = try sc.jcs.encode(alloc, msg_v);
    defer alloc.free(jcs_bytes);

    const sig = try sc.signing.signString(alloc, sc.foundation.SEED, jcs_bytes);
    defer alloc.free(sig);
    try std.testing.expectEqualStrings(
        "ed25519:XQhbaHNNhD9hCbkfmtnVZsUZz0pd7CSLuG1qCM8Rq2FF51qUZSpXh5Wg2DVGWYeD1ZHH9D2m0HKIlN9U57U4CA==",
        sig,
    );
}

test "go attestation signature byte-matches go.json" {
    const alloc = std.testing.allocator;
    // Get exact `cid` and `contractSetCid` from go.json.
    var ob = ObjectBuilder.init(alloc);
    try ob.add("schemaVersion", try Value.newString(alloc, "1"));
    try ob.add("kind", try Value.newString(alloc, "self-contracts-attestation"));
    try ob.add("lang", try Value.newString(alloc, "go"));
    // The fields below come from the actual go.json file. We populate them
    // dynamically at test time via @embedFile on the attestation JSON.
    const go_json = @embedFile("test_fixtures/go_attestation.json");
    const Parsed = struct {
        schemaVersion: []const u8,
        kind: []const u8,
        lang: []const u8,
        cid: []const u8,
        contractSetCid: []const u8,
        declaredAt: []const u8,
        signer: []const u8,
        signature: []const u8,
    };
    const parsed = try std.json.parseFromSlice(Parsed, alloc, go_json, .{});
    defer parsed.deinit();
    try ob.add("cid", try Value.newString(alloc, parsed.value.cid));
    try ob.add("contractSetCid", try Value.newString(alloc, parsed.value.contractSetCid));
    try ob.add("declaredAt", try Value.newString(alloc, parsed.value.declaredAt));
    try ob.add("signer", try Value.newString(alloc, parsed.value.signer));
    const msg_v = try ob.finish();
    defer {
        msg_v.deinit(alloc);
        alloc.destroy(msg_v);
    }
    const jcs_bytes = try sc.jcs.encode(alloc, msg_v);
    defer alloc.free(jcs_bytes);

    const sig = try sc.signing.signString(alloc, sc.foundation.SEED, jcs_bytes);
    defer alloc.free(sig);
    try std.testing.expectEqualStrings(parsed.value.signature, sig);
}
