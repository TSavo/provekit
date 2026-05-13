# SPDX-License-Identifier: Apache-2.0
#
# BLAKE3-512 hash tests. Cross-kit byte-equivalence pinned against
# the Rust + Python kits (both produce 64-byte BLAKE3-512 output for
# canonical bytes; this kit binds to libblake3 directly).

require "minitest/autorun"
require_relative "../lib/provekit/blake3"

class TestBlake3 < Minitest::Test
  B = Provekit::Blake3

  # Known vector: BLAKE3 of UTF-8 "hello" with 64-byte XOF output.
  # Verified against:
  #   - Python: blake3.blake3(b"hello").digest(length=64).hex()
  #   - Rust:   provekit_canonicalizer::blake3_512_of("hello")
  HELLO_HEX_64 =
    "ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f" \
    "e992405f0d785b599a2e3387f6d34d01faccfeb22fb697ef3fd53541241a338c"

  # Known vector: empty input (BLAKE3 of "").
  EMPTY_HEX_64 =
    "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262" \
    "e00f03e7b69af26b7faaf09fcd333050338ddfe085b8cc869ca98b206c08243a"

  def test_hello_hex_matches_known_vector
    assert_equal "blake3-512:#{HELLO_HEX_64}", B.hex("hello")
  end

  def test_empty_hex_matches_known_vector
    assert_equal "blake3-512:#{EMPTY_HEX_64}", B.hex("")
  end

  def test_bytes_returns_64_bytes
    assert_equal 64, B.bytes("hello").bytesize
  end

  def test_bytes_returns_binary_string
    assert_equal Encoding::ASCII_8BIT, B.bytes("hello").encoding
  end

  def test_hex_starts_with_self_identifying_prefix
    assert B.hex("anything").start_with?("blake3-512:")
  end

  def test_hex_total_length_is_prefix_plus_128
    assert_equal "blake3-512:".length + 128, B.hex("anything").length
  end

  def test_deterministic
    a = B.hex("data")
    b = B.hex("data")
    assert_equal a, b
  end

  def test_different_inputs_produce_different_hashes
    refute_equal B.hex("a"), B.hex("b")
  end

  def test_handles_binary_data
    binary = (0..255).to_a.pack("C*")
    out = B.hex(binary)
    assert_equal "blake3-512:".length + 128, out.length
  end
end
