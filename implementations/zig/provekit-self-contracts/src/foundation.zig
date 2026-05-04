// SPDX-License-Identifier: Apache-2.0
//
// Foundation v0 ed25519 seed — the cross-kit anchor for substrate
// signing. Documented as a deterministic test seed; v1 is HSM-generated.
// A signed catalog under this seed is structurally valid but offers no
// trust beyond "the bytes match the public seed in the repo."
//
// Mirrors `tools/foundation-keygen/src/lib.rs` const FOUNDATION_V0_SEED.

const signing = @import("signing.zig");

/// 32-byte seed shared across all 11 ProvekIt kits.
pub const SEED: signing.Seed = @splat(0x42);
