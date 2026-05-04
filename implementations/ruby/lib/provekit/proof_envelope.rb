# SPDX-License-Identifier: Apache-2.0
#
# Proof-envelope builder. Deterministic CBOR (RFC 8949 §4.2.1) encoding
# of a catalog memento with member envelopes embedded as opaque byte
# strings, signed with Ed25519.
#
# Protocol:
#   1. Build the unsigned body as a CBOR map; map keys are sorted by
#      bytewise lex order of their CBOR-encoded form (RFC 8949 §4.2.1).
#   2. Ed25519-sign the unsigned-body bytes.
#   3. Re-emit the body with the +signature+ key added; canonical sort
#      slots it in by lex order automatically.
#   4. BLAKE3-512 the final bytes; the full self-identifying string
#      "blake3-512:<128 hex>" IS the catalog CID.
#
# The +members+ map key is the embedded envelope's own CID, and the
# value is its canonical bytes (JCS-JSON for memento envelopes per the
# memento envelope grammar) encoded as a CBOR byte string.
#
# Reference (authoritative):
#   implementations/rust/provekit-proof-envelope/src/proof.rs
#   protocol/specs/2026-04-30-proof-file-format.md
#
# Cross-kit byte-equivalence:
#   The Ruby output is byte-identical to the Rust + Python kits for
#   the same canonical input. See test/test_proof_envelope.rb.

require_relative "blake3"
require_relative "cbor"
require_relative "signing"

module Provekit
  module ProofEnvelope
    # Logical input shape for a .proof catalog memento.
    # Mirrors +ProofEnvelopeInput+ in
    # implementations/rust/provekit-proof-envelope/src/proof.rs.
    #
    # members: hash from member CID (e.g. "blake3-512:<hex>") to that
    #          member's canonical bytes (JCS-JSON for memento envelopes).
    # signer_seed: 32-byte Ed25519 seed. Defaults to FOUNDATION_V0_SEED
    #              (the publicly-known test key used by all other kits).
    Input = Struct.new(
      :name,
      :version,
      :members,
      :signer_cid,
      :declared_at,
      :signer_seed,
      :binary_cid,
      :metadata,
      keyword_init: true,
    ) do
      def initialize(name:, version:, members:, signer_cid:, declared_at:,
                     signer_seed: Provekit::Signing::FOUNDATION_V0_SEED,
                     binary_cid: nil, metadata: nil)
        super
      end
    end

    # Result of {build_proof_envelope}.
    #
    # +bytes+: CBOR bytes of the signed catalog. The BLAKE3-512 hash of
    #          these bytes IS the catalog CID.
    # +cid+:   Full self-identifying CID, e.g. "blake3-512:<128 hex>".
    #          This string is the .proof filename without extension.
    Output = Struct.new(:bytes, :cid, keyword_init: true)

    # One CBOR (key, value) pair, with its key pre-encoded so the
    # outer map can sort by bytewise CBOR-encoded-key form.
    Pair = Struct.new(:key_cbor, :value_cbor)

    # ── Internals ────────────────────────────────────────────────

    def self.make_string_pair(key, value)
      v = []
      Cbor.encode_tstr(v, value.to_s)
      Pair.new(Cbor.encode_key(key), v)
    end

    def self.make_bytes_pair(key, value)
      v = []
      Cbor.encode_bstr(v, value)
      Pair.new(Cbor.encode_key(key), v)
    end

    def self.make_members_pair(key, members)
      # Encode as { tstr(cid) => bstr(envelope-bytes) }, sort by
      # bytewise CBOR-encoded-key form.
      pairs = members.map { |cid, bytes_val| make_bytes_pair(cid, bytes_val) }
      value = []
      emit_sorted_map(value, pairs)
      Pair.new(Cbor.encode_key(key), value)
    end

    def self.emit_sorted_map(out, pairs)
      pairs.sort_by!(&:key_cbor)
      Cbor.encode_map_head(out, pairs.length)
      pairs.each do |p|
        out.concat(p.key_cbor)
        out.concat(p.value_cbor)
      end
    end

    def self.body_pairs_unsigned(input)
      pairs = [
        make_string_pair("kind",       "catalog"),
        make_string_pair("name",       input.name),
        make_string_pair("version",    input.version),
        make_members_pair("members",   input.members),
        make_string_pair("signer",     input.signer_cid),
        make_string_pair("declaredAt", input.declared_at),
      ]
      pairs << make_string_pair("binaryCid", input.binary_cid) if input.binary_cid
      if input.metadata
        meta_pairs = input.metadata.map { |k, v| make_string_pair(k.to_s, v.to_s) }
        meta_value = []
        emit_sorted_map(meta_value, meta_pairs)
        pairs << Pair.new(Cbor.encode_key("metadata"), meta_value)
      end
      pairs
    end

    # ── Public API ───────────────────────────────────────────────

    # Build a .proof envelope from +input+ (an Input struct).
    # Output is byte-identical to the Rust + Python kits' build for the
    # same canonical input. See module docstring for the four-step protocol.
    def self.build(input)
      # Step 1: encode unsigned body with sorted keys.
      unsigned_pairs = body_pairs_unsigned(input)
      unsigned_bytes_arr = []
      emit_sorted_map(unsigned_bytes_arr, unsigned_pairs)
      unsigned_bytes = Cbor.pack_bytes(unsigned_bytes_arr)

      # Step 2: ed25519-sign the unsigned bytes.
      sig = Signing.ed25519_sign_with_seed(input.signer_seed, unsigned_bytes)

      # Step 3: re-emit with signature added; keys re-sort automatically.
      signed_pairs = body_pairs_unsigned(input)
      signed_pairs << make_bytes_pair("signature", sig)
      final_arr = []
      emit_sorted_map(final_arr, signed_pairs)
      final_bytes = Cbor.pack_bytes(final_arr)

      # Step 4: filename CID = full self-identifying BLAKE3-512.
      cid = Blake3.hex(final_bytes)
      Output.new(bytes: final_bytes, cid: cid)
    end

    # Verify a .proof envelope against +expected_cid+ and +signer_pubkey+.
    #
    # Checks:
    #   1. CID matches: BLAKE3-512(proof_bytes) == expected_cid.
    #   2. CBOR decodes to a catalog map with the required keys.
    #   3. Ed25519 signature verifies: re-encode the unsigned body
    #      (all keys except +signature+) and verify the embedded
    #      +signature+ bytes against +signer_pubkey+ (raw 32-byte
    #      Ed25519 public key).
    #
    # +signer_pubkey+ is the raw 32-byte public key (NOT the seed /
    # private key). Use {Provekit::Signing.ed25519_pubkey_bytes} to
    # derive it from a seed if needed.
    #
    # Returns true iff all three checks pass.
    def self.verify(proof_bytes, expected_cid, signer_pubkey)
      return false unless signer_pubkey.is_a?(String) && signer_pubkey.bytesize == 32

      # Check 1: CID
      actual_cid = Blake3.hex(proof_bytes)
      return false unless actual_cid == expected_cid

      # Check 2: decode + shape
      catalog = decode_signed_catalog(proof_bytes)
      return false unless catalog
      return false unless catalog["kind"] == "catalog"
      required = %w[kind name version members signer declaredAt signature]
      return false unless required.all? { |k| catalog.key?(k) }
      sig_bytes = catalog["signature"]
      return false unless sig_bytes.is_a?(String) && sig_bytes.bytesize == 64

      # Check 3: re-encode the unsigned body and verify the signature.
      input = Input.new(
        name:        catalog["name"],
        version:     catalog["version"],
        members:     catalog["members"] || {},
        signer_cid:  catalog["signer"],
        declared_at: catalog["declaredAt"],
        signer_seed: Signing::FOUNDATION_V0_SEED, # unused (we re-emit unsigned only)
        binary_cid:  catalog["binaryCid"],
        metadata:    catalog["metadata"],
      )
      unsigned_pairs = body_pairs_unsigned(input)
      unsigned_arr = []
      emit_sorted_map(unsigned_arr, unsigned_pairs)
      unsigned_bytes = Cbor.pack_bytes(unsigned_arr)

      Signing.verify_raw(signer_pubkey, sig_bytes, unsigned_bytes)
    end

    # ── Minimal CBOR decoder (verify-only path) ───────────────────
    #
    # We only need to decode the proof envelope shape we ourselves emit:
    # outer map with keys (tstr): kind, name, version, members, signer,
    # declaredAt, signature, optional binaryCid, optional metadata.
    # Values are tstr, bstr, or a nested {tstr -> bstr} map (for
    # +members+) / {tstr -> tstr} map (for +metadata+).

    def self.decode_signed_catalog(bytes)
      io = StringIO.new(bytes.b)
      io.binmode if io.respond_to?(:binmode)
      val = decode_value(io)
      return nil unless val.is_a?(Hash)
      val
    rescue StandardError
      nil
    end

    def self.read_head(io)
      first = io.read(1)
      raise "EOF" unless first
      ib = first.unpack1("C")
      mt  = ib >> 5
      arg = ib & 0x1F
      arg =
        case arg
        when 0..23 then arg
        when 24    then io.read(1).unpack1("C")
        when 25    then io.read(2).unpack1("n")
        when 26    then io.read(4).unpack1("N")
        when 27    then io.read(8).unpack1("Q>")
        else raise "unsupported CBOR additional info: #{arg}"
        end
      [mt, arg]
    end

    def self.decode_value(io)
      mt, arg = read_head(io)
      case mt
      when Cbor::UNSIGNED_INT then arg
      when Cbor::BYTE_STRING  then io.read(arg).b
      when Cbor::TEXT_STRING  then io.read(arg).force_encoding(Encoding::UTF_8)
      when Cbor::ARRAY
        Array.new(arg) { decode_value(io) }
      when Cbor::MAP
        h = {}
        arg.times do
          k = decode_value(io)
          v = decode_value(io)
          h[k] = v
        end
        h
      else
        raise "unsupported major type #{mt}"
      end
    end
  end
end

require "stringio"
