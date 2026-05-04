# SPDX-License-Identifier: Apache-2.0
#
# Ed25519 signing tests. Determinism + known-vector parity with the
# Rust kit (ed25519-dalek) and Python kit (PyNaCl) — all three
# implement RFC 8032 Ed25519 and produce byte-identical signatures
# for the same (seed, message) pair.

require "minitest/autorun"
require "base64"
require_relative "../lib/provekit/signing"

class TestSigning < Minitest::Test
  S = Provekit::Signing

  SEED_42 = ([0x42] * 32).pack("C*").freeze
  SEED_99 = ([0x99] * 32).pack("C*").freeze

  # Known-good vectors. Pinned against PyNaCl + ed25519-dalek output.
  PUBKEY_42_HEX = "2152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db12"
  SIG_42_HELLO_HEX =
    "fa10ea646d7ee80994bdddd03942479b61a9d54962cffe3e629537266b8adc46" \
    "b5a60b204f1798bf32398bc2d4ef8d791a4e4ae7c39eb43e83563ce67d39e405"

  def test_deterministic_signature_for_fixed_seed
    a = S.ed25519_sign_with_seed(SEED_42, "hello")
    b = S.ed25519_sign_with_seed(SEED_42, "hello")
    assert_equal a, b
  end

  def test_signature_is_64_bytes
    sig = S.ed25519_sign_with_seed(SEED_42, "hello")
    assert_equal 64, sig.bytesize
  end

  def test_known_vector_matches_pynacl_dalek
    # Cross-kit pin: Ruby ed25519 gem must match PyNaCl + ed25519-dalek
    # for seed=[0x42]*32, message="hello".
    sig = S.ed25519_sign_with_seed(SEED_42, "hello")
    assert_equal SIG_42_HELLO_HEX, sig.unpack1("H*")
  end

  def test_pubkey_bytes_match_known_vector
    pk = S.ed25519_pubkey_bytes(SEED_42)
    assert_equal PUBKEY_42_HEX, pk.unpack1("H*")
  end

  def test_sign_string_has_prefix
    s = S.ed25519_sign_string(SEED_42, "hello")
    assert s.start_with?("ed25519:")
  end

  def test_pubkey_string_has_prefix
    pk = S.ed25519_pubkey_string(SEED_42)
    assert pk.start_with?("ed25519:")
  end

  def test_pubkey_base64_is_44_chars
    # 32 bytes -> 44 base64 chars (with stdpad).
    pk = S.ed25519_pubkey_string(SEED_42)
    b64 = pk["ed25519:".length..]
    assert_equal 44, b64.length
  end

  def test_verify_round_trip
    pk  = S.ed25519_pubkey_string(SEED_42)
    sig = S.ed25519_sign_string(SEED_42, "hello world")
    assert S.ed25519_verify_string(pk, sig, "hello world")
    refute S.ed25519_verify_string(pk, sig, "goodbye world")
  end

  def test_verify_rejects_malformed_inputs
    refute S.ed25519_verify_string("not-prefixed", "ed25519:AAAA==", "x")
    refute S.ed25519_verify_string("ed25519:AAAA==", "not-prefixed", "x")
    refute S.ed25519_verify_string("ed25519:!!!!", "ed25519:!!!!", "x")
  end

  def test_foundation_v0_seed_is_42_repeated
    assert_equal SEED_42, S::FOUNDATION_V0_SEED
    assert_equal 32, S::FOUNDATION_V0_SEED.bytesize
  end

  def test_different_seeds_produce_different_signatures
    sig_a = S.ed25519_sign_with_seed(SEED_42, "same message")
    sig_b = S.ed25519_sign_with_seed(SEED_99, "same message")
    refute_equal sig_a, sig_b
  end

  def test_different_messages_produce_different_signatures
    sig_a = S.ed25519_sign_with_seed(SEED_42, "message a")
    sig_b = S.ed25519_sign_with_seed(SEED_42, "message b")
    refute_equal sig_a, sig_b
  end

  def test_verify_raw_round_trip
    pk_bytes = S.ed25519_pubkey_bytes(SEED_42)
    sig = S.ed25519_sign_with_seed(SEED_42, "hello")
    assert S.verify_raw(pk_bytes, sig, "hello")
    refute S.verify_raw(pk_bytes, sig, "world")
  end

  def test_invalid_seed_length_raises
    assert_raises(ArgumentError) do
      S.ed25519_sign_with_seed("short", "hello")
    end
    assert_raises(ArgumentError) do
      S.ed25519_pubkey_bytes("short")
    end
  end
end
