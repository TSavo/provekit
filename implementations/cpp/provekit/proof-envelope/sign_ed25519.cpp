// SPDX-License-Identifier: Apache-2.0
//
// ed25519 signing via OpenSSL EVP_PKEY_ED25519.

#include "sign_ed25519.hpp"

#include <openssl/evp.h>
#include <memory>
#include <stdexcept>

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

}  // namespace provekit::proof_envelope
