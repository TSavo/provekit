# SPDX-License-Identifier: Apache-2.0
#
# Ed25519 signing helper. v1.1.0 of the protocol mandates self-identifying
# signatures of the form:
#
#   "ed25519:" + base64-stdpad(64-byte-signature)
#
# And self-identifying public keys in the same form. The .proof file
# envelope itself stores its catalog signature as a RAW 64-byte CBOR
# byte string (not the prefixed string form): only the per-memento
# `producerSignature` field uses the prefixed string form, because
# memento envelopes are JCS-JSON.
#
# Backed by the +ed25519+ gem (Ruby ed25519 binding). Byte-equivalent
# to the Rust ed25519-dalek peer and to PyNaCl: all three implement
# RFC 8032 Ed25519.
#
# Reference (authoritative):
#   implementations/rust/provekit-proof-envelope/src/sign.rs
#   implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/signing.py

require "base64"
require "ed25519"

module Provekit
  module Signing
    ED25519_SIG_PREFIX = "ed25519:"
    ED25519_KEY_PREFIX = "ed25519:"

    # Foundation v0 seed: publicly-known, deterministic test seed.
    # Documented as a test seed; v1 is HSM-generated.
    # Source: tools/foundation-keygen/src/lib.rs FOUNDATION_V0_SEED.
    FOUNDATION_V0_SEED = ([0x42] * 32).pack("C*").freeze

    # Sign +message+ with the Ed25519 private key derived from +seed+.
    # +seed+ must be exactly 32 bytes (RFC 8032 §5.1.5 seed format).
    # Returns the raw 64-byte signature. Byte-identical to the Rust
    # +ed25519_sign_with_seed+ for the same (seed, message) pair.
    def self.ed25519_sign_with_seed(seed, message)
      raise ArgumentError, "seed must be exactly 32 bytes" unless seed.bytesize == 32
      sk = ::Ed25519::SigningKey.new(seed)
      sk.sign(message.to_s.b)
    end

    # Sign and return the spec's self-identifying string form.
    # Format: "ed25519:" + base64-stdpad(64-byte-signature).
    def self.ed25519_sign_string(seed, message)
      sig = ed25519_sign_with_seed(seed, message)
      ED25519_SIG_PREFIX + Base64.strict_encode64(sig)
    end

    # Derive the public key from +seed+ and return the self-identifying
    # string form: "ed25519:" + base64-stdpad(32-byte-pubkey).
    def self.ed25519_pubkey_string(seed)
      raise ArgumentError, "seed must be exactly 32 bytes" unless seed.bytesize == 32
      sk = ::Ed25519::SigningKey.new(seed)
      ED25519_KEY_PREFIX + Base64.strict_encode64(sk.verify_key.to_bytes)
    end

    # Raw 32-byte public key derived from +seed+.
    def self.ed25519_pubkey_bytes(seed)
      raise ArgumentError, "seed must be exactly 32 bytes" unless seed.bytesize == 32
      ::Ed25519::SigningKey.new(seed).verify_key.to_bytes
    end

    # Verify +message+ against +sig_string+ using +pubkey_string+.
    # Both strings use the spec's self-identifying form
    # ("ed25519:" + base64-stdpad(bytes)).
    #
    # Returns true iff the signature is valid. Returns false for any
    # malformed input rather than raising, so the caller's fast-path
    # and verify-failed-path stay separate.
    def self.ed25519_verify_string(pubkey_string, sig_string, message)
      return false unless pubkey_string.is_a?(String) && pubkey_string.start_with?(ED25519_KEY_PREFIX)
      return false unless sig_string.is_a?(String) && sig_string.start_with?(ED25519_SIG_PREFIX)

      begin
        pk_bytes = Base64.strict_decode64(pubkey_string[ED25519_KEY_PREFIX.length..])
        sig_bytes = Base64.strict_decode64(sig_string[ED25519_SIG_PREFIX.length..])
      rescue ArgumentError
        return false
      end

      return false unless pk_bytes.bytesize == 32 && sig_bytes.bytesize == 64

      verify_raw(pk_bytes, sig_bytes, message.to_s.b)
    end

    # Verify a raw 64-byte signature against a raw 32-byte public key.
    # Returns true / false. Never raises on a malformed input.
    def self.verify_raw(pubkey_bytes, sig_bytes, message)
      return false unless pubkey_bytes.is_a?(String) && pubkey_bytes.bytesize == 32
      return false unless sig_bytes.is_a?(String) && sig_bytes.bytesize == 64
      vk = ::Ed25519::VerifyKey.new(pubkey_bytes)
      vk.verify(sig_bytes, message.to_s.b)
    rescue ::Ed25519::VerifyError
      false
    rescue StandardError
      false
    end

    # ------------------------------------------------------------------
    # Signer: minimal handle bundling per-actor stable fields.
    #
    # Used by Provekit::ClaimEnvelope.from_contract_decl(decl, signer).
    # Only the fields stable per-actor live on the Signer; per-attestation
    # fields (produced_at, authoring, input_cids) are passed at call time.
    #
    # The producer-id ("ruby-kit@1.0", "rust-test@1.0", etc.) is bound
    # into the contract's bindingHash (per `mint_contract` derivation
    # `bindingHash = hash(JCS({producerId, contractName, propertyHash}))`)
    # so it lives on the Signer rather than being recomputed at every
    # call.
    # ------------------------------------------------------------------
    class Signer
      attr_reader :seed, :producer_id

      def initialize(seed:, producer_id:)
        unless seed.is_a?(String) && seed.bytesize == 32
          raise ArgumentError, "Signer seed must be exactly 32 bytes"
        end
        unless producer_id.is_a?(String) && !producer_id.empty?
          raise ArgumentError, "Signer producer_id must be a non-empty String"
        end
        @seed = seed.b.freeze
        @producer_id = producer_id.dup.freeze
      end

      # Convenience: a Signer using the public foundation v0 test seed.
      # Foundation v0 is the publicly-known cross-kit test seed; it is
      # appropriate for fixtures and conformance tests, not production.
      def self.foundation_v0(producer_id: "ruby-kit@1.0")
        new(seed: FOUNDATION_V0_SEED, producer_id: producer_id)
      end

      # Self-identifying public-key string: "ed25519:<base64>".
      def pubkey_string
        Provekit::Signing.ed25519_pubkey_string(@seed)
      end

      # Build a v1.2-layered ClaimEnvelope from a `ContractDecl`.
      # Convenience delegate to `Provekit::ClaimEnvelope.from_contract_decl`.
      # Loaded lazily to avoid the signing -> claim_envelope -> signing
      # require cycle.
      def sign_claim(decl, produced_at:, authoring: nil, input_cids: nil)
        require_relative "claim_envelope"
        Provekit::ClaimEnvelope.from_contract_decl(
          decl, self,
          produced_at: produced_at,
          authoring: authoring,
          input_cids: input_cids,
        )
      end
    end
  end
end
