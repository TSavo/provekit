// SPDX-License-Identifier: Apache-2.0
//
// ed25519 signing via OpenSSL EVP_PKEY_ED25519.

#include "sign_ed25519.hpp"

#include <openssl/evp.h>
#include <memory>
#include <stdexcept>
#include <string>

namespace provekit::proof_envelope {

namespace {

struct EvpPkeyDeleter {
    void operator()(EVP_PKEY* p) const noexcept { if (p) EVP_PKEY_free(p); }
};
struct EvpMdCtxDeleter {
    void operator()(EVP_MD_CTX* c) const noexcept { if (c) EVP_MD_CTX_free(c); }
};

}  // namespace

Ed25519Signature ed25519_sign_with_seed(
    const Ed25519Seed& seed,
    const uint8_t* message,
    size_t message_len) {
    EVP_PKEY* raw_key = EVP_PKEY_new_raw_private_key(
        EVP_PKEY_ED25519, nullptr, seed.data(), seed.size());
    if (!raw_key) {
        throw std::runtime_error("EVP_PKEY_new_raw_private_key failed");
    }
    std::unique_ptr<EVP_PKEY, EvpPkeyDeleter> pkey(raw_key);

    std::unique_ptr<EVP_MD_CTX, EvpMdCtxDeleter> ctx(EVP_MD_CTX_new());
    if (!ctx) throw std::runtime_error("EVP_MD_CTX_new failed");

    if (EVP_DigestSignInit(ctx.get(), nullptr, nullptr, nullptr, pkey.get()) != 1) {
        throw std::runtime_error("EVP_DigestSignInit failed");
    }

    Ed25519Signature sig{};
    size_t sig_len = sig.size();
    if (EVP_DigestSign(ctx.get(), sig.data(), &sig_len, message, message_len) != 1) {
        throw std::runtime_error("EVP_DigestSign failed");
    }
    if (sig_len != sig.size()) {
        throw std::runtime_error("ed25519 signature length unexpected");
    }
    return sig;
}

Ed25519PublicKey ed25519_pubkey_from_seed(const Ed25519Seed& seed) {
    EVP_PKEY* raw_key = EVP_PKEY_new_raw_private_key(
        EVP_PKEY_ED25519, nullptr, seed.data(), seed.size());
    if (!raw_key) {
        throw std::runtime_error("EVP_PKEY_new_raw_private_key (pubkey extraction) failed");
    }
    std::unique_ptr<EVP_PKEY, EvpPkeyDeleter> pkey(raw_key);

    Ed25519PublicKey pub{};
    size_t pub_len = pub.size();
    if (EVP_PKEY_get_raw_public_key(pkey.get(), pub.data(), &pub_len) != 1) {
        throw std::runtime_error("EVP_PKEY_get_raw_public_key failed");
    }
    if (pub_len != pub.size()) {
        throw std::runtime_error("ed25519 public key length unexpected");
    }
    return pub;
}

namespace {

// Standard base64 (RFC 4648 §4); produces the same string node's
// crypto / Rust base64 STANDARD engine / NSec produce, ensuring the
// `ed25519:<base64>` self-identifying form is byte-identical across peers.
std::string base64_encode_pubkey(const Ed25519PublicKey& bytes) {
    static const char alphabet[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string out;
    out.reserve(((bytes.size() + 2) / 3) * 4);
    size_t i = 0;
    for (; i + 3 <= bytes.size(); i += 3) {
        uint32_t triple = (uint32_t(bytes[i]) << 16)
                        | (uint32_t(bytes[i + 1]) << 8)
                        |  uint32_t(bytes[i + 2]);
        out.push_back(alphabet[(triple >> 18) & 0x3F]);
        out.push_back(alphabet[(triple >> 12) & 0x3F]);
        out.push_back(alphabet[(triple >> 6) & 0x3F]);
        out.push_back(alphabet[triple & 0x3F]);
    }
    if (i < bytes.size()) {
        uint32_t triple = uint32_t(bytes[i]) << 16;
        if (i + 1 < bytes.size()) triple |= uint32_t(bytes[i + 1]) << 8;
        out.push_back(alphabet[(triple >> 18) & 0x3F]);
        out.push_back(alphabet[(triple >> 12) & 0x3F]);
        out.push_back((i + 1 < bytes.size()) ? alphabet[(triple >> 6) & 0x3F] : '=');
        out.push_back('=');
    }
    return out;
}

}  // namespace

std::string ed25519_pubkey_string_from_seed(const Ed25519Seed& seed) {
    auto pub = ed25519_pubkey_from_seed(seed);
    return std::string("ed25519:") + base64_encode_pubkey(pub);
}

}  // namespace provekit::proof_envelope
