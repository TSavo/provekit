// SPDX-License-Identifier: Apache-2.0
//
// ed25519 signing wrapper. Uses OpenSSL EVP_PKEY_ED25519 (RFC 8032).
// Same algorithm + key encoding the TypeScript reference uses
// (node:crypto's ed25519 path also calls OpenSSL underneath), so
// signatures are byte-identical given byte-identical (seed, message).

#pragma once

#include <array>
#include <cstdint>
#include <string>

namespace provekit::proof_envelope {

using Ed25519Seed = std::array<uint8_t, 32>;
using Ed25519Signature = std::array<uint8_t, 64>;
using Ed25519PublicKey = std::array<uint8_t, 32>;

// Derive the 32-byte ed25519 public key from a 32-byte seed. Same
// derivation node:crypto / NSec / Rust ed25519-dalek perform; the
// resulting public key bytes are byte-identical across implementations.
// Throws std::runtime_error on OpenSSL failure.
Ed25519PublicKey ed25519_pubkey_from_seed(const Ed25519Seed& seed);

// Self-identifying public key string: `"ed25519:" + base64(pubkey_bytes)`.
// Matches the format Rust + Go + TS + C# peers produce.
std::string ed25519_pubkey_string_from_seed(const Ed25519Seed& seed);

// Sign `message` with the private key derived from `seed`. The
// resulting signature is the canonical 64-byte ed25519 RFC 8032
// output (R || S). Throws std::runtime_error on OpenSSL failure.
Ed25519Signature ed25519_sign_with_seed(
    const Ed25519Seed& seed,
    const uint8_t* message,
    size_t message_len);

inline Ed25519Signature ed25519_sign_with_seed(
    const Ed25519Seed& seed, const std::string& message) {
    return ed25519_sign_with_seed(
        seed,
        reinterpret_cast<const uint8_t*>(message.data()),
        message.size());
}

}  // namespace provekit::proof_envelope
