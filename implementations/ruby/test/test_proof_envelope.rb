# SPDX-License-Identifier: Apache-2.0
#
# Proof-envelope tests: round-trip, cross-kit byte-equivalence, and
# sign/verify with known-good test vectors.
#
# Cross-kit byte-equivalence pins are derived from the Rust kit. The
# Python kit (PR #221) ALSO matches this pin; running the Python kit
# locally with identical input reproduces the same bytes. So Ruby
# matching this pin = Ruby matching Rust = Ruby matching Python.
#
# If a pin fails, the mismatch is a real divergence: surface it,
# don't paper over it.

require "minitest/autorun"
require_relative "../lib/provekit/blake3"
require_relative "../lib/provekit/cbor"
require_relative "../lib/provekit/signing"
require_relative "../lib/provekit/proof_envelope"

class TestProofEnvelope < Minitest::Test
  PE      = Provekit::ProofEnvelope
  SIGNING = Provekit::Signing
  BLAKE3  = Provekit::Blake3

  FOUNDATION_PUBKEY = SIGNING.ed25519_pubkey_bytes(SIGNING::FOUNDATION_V0_SEED).freeze

  # ── Cross-kit byte-equivalence pin (from Rust + Python kits) ──
  # Source:
  #   cargo run --release -p provekit-proof-envelope --example proof_envelope_bytes
  #   (And reproduced byte-for-byte by the Python kit per PR #221.)
  RUST_FIXTURE_CID =
    "blake3-512:5ed1e1f705622ad52ae4683e3d12df5586d364d66bb3186f5be512415edf290" \
    "844d74e73a2857cd858f37803e4b11fe5c7cba7884caa6b9ff847521ce32ea056"

  RUST_FIXTURE_BYTES_HEX =
    "a7646b696e6467636174616c6f67646e616d656940746573742f636174667369676e65" \
    "726d626c616b65332d3531323a6363676d656d62657273a26d626c616b65332d353132" \
    "3a6161517b2268656c6c6f223a22776f726c64227d6d626c616b65332d3531323a6262" \
    "537b22676f6f64627965223a22776f726c64227d6776657273696f6e65312e302e3069" \
    "7369676e617475726558406a21dd428a54e22c82ca6d6125a7293c4a723786cb1840e8" \
    "91cefa03e63246eb97ef13dab86b7b1469d67302fadc969cd88c92c29495d13c75fc02" \
    "01a7263b066a6465636c6172656441747818323032362d30342d33305430303a30303a" \
    "30302e3030305a"

  # ── Fixtures ────────────────────────────────────────────────

  def minimal_input(seed: SIGNING::FOUNDATION_V0_SEED)
    PE::Input.new(
      name:        "@test/cat",
      version:     "1.0.0",
      members:     { "blake3-512:aa" => '{"hello":"world"}' },
      signer_cid:  "blake3-512:cc",
      declared_at: "2026-04-30T00:00:00.000Z",
      signer_seed: seed,
    )
  end

  def two_member_input
    PE::Input.new(
      name:        "@test/cat",
      version:     "1.0.0",
      members:     {
        "blake3-512:aa" => '{"hello":"world"}',
        "blake3-512:bb" => '{"goodbye":"world"}',
      },
      signer_cid:  "blake3-512:cc",
      declared_at: "2026-04-30T00:00:00.000Z",
      signer_seed: SIGNING::FOUNDATION_V0_SEED,
    )
  end

  # ── Round-trip ──────────────────────────────────────────────

  def test_build_then_verify
    out = PE.build(minimal_input)
    assert PE.verify(out.bytes, out.cid, FOUNDATION_PUBKEY)
  end

  def test_cid_is_blake3_512_of_bytes
    out = PE.build(minimal_input)
    assert_equal BLAKE3.hex(out.bytes), out.cid
  end

  def test_cid_has_correct_prefix
    out = PE.build(minimal_input)
    assert out.cid.start_with?("blake3-512:")
  end

  def test_cid_length_is_prefix_plus_128_hex
    out = PE.build(minimal_input)
    assert_equal "blake3-512:".length + 128, out.cid.length
  end

  def test_signed_map_head_is_seven_keys
    # 7-key map head: major 5 (0xA0) + count 7 = 0xA7.
    out = PE.build(minimal_input)
    assert_equal 0xA7, out.bytes.bytes.first
  end

  def test_deterministic_across_runs
    a = PE.build(minimal_input)
    b = PE.build(minimal_input)
    assert_equal a.bytes, b.bytes
    assert_equal a.cid, b.cid
  end

  def test_two_members_round_trips
    out = PE.build(two_member_input)
    assert PE.verify(out.bytes, out.cid, FOUNDATION_PUBKEY)
  end

  def test_changing_name_changes_cid
    a = PE.build(minimal_input)
    b = PE.build(PE::Input.new(
      name:        "@other/name",
      version:     "1.0.0",
      members:     { "blake3-512:aa" => '{"hello":"world"}' },
      signer_cid:  "blake3-512:cc",
      declared_at: "2026-04-30T00:00:00.000Z",
    ))
    refute_equal a.cid, b.cid
  end

  def test_changing_seed_changes_cid
    a = PE.build(minimal_input(seed: ([0x42] * 32).pack("C*")))
    b = PE.build(minimal_input(seed: ([0x99] * 32).pack("C*")))
    refute_equal a.cid, b.cid
  end

  def test_verify_rejects_tampered_cid
    out = PE.build(minimal_input)
    fake_cid = "blake3-512:" + ("00" * 64)
    refute PE.verify(out.bytes, fake_cid, FOUNDATION_PUBKEY)
  end

  def test_verify_rejects_wrong_pubkey
    out = PE.build(minimal_input)
    wrong_pubkey = SIGNING.ed25519_pubkey_bytes(([0x99] * 32).pack("C*"))
    refute PE.verify(out.bytes, out.cid, wrong_pubkey)
  end

  def test_binary_cid_field_included_in_signed_body
    inp = PE::Input.new(
      name:        "@test/cat",
      version:     "1.0.0",
      members:     { "blake3-512:aa" => "data" },
      signer_cid:  "blake3-512:cc",
      declared_at: "2026-04-30T00:00:00.000Z",
      binary_cid:  "blake3-512:deadbeef",
    )
    out = PE.build(inp)
    assert PE.verify(out.bytes, out.cid, FOUNDATION_PUBKEY)

    inp_no_binary = PE::Input.new(
      name:        "@test/cat",
      version:     "1.0.0",
      members:     { "blake3-512:aa" => "data" },
      signer_cid:  "blake3-512:cc",
      declared_at: "2026-04-30T00:00:00.000Z",
    )
    out_no_binary = PE.build(inp_no_binary)
    refute_equal out.cid, out_no_binary.cid
  end

  def test_metadata_field_included_in_signed_body
    inp = PE::Input.new(
      name:        "@test/cat",
      version:     "1.0.0",
      members:     { "blake3-512:aa" => "data" },
      signer_cid:  "blake3-512:cc",
      declared_at: "2026-04-30T00:00:00.000Z",
      metadata:    { "tool" => "ruby-kit", "version" => "0.1.0" },
    )
    out = PE.build(inp)
    assert PE.verify(out.bytes, out.cid, FOUNDATION_PUBKEY)
  end

  # ── Cross-kit byte-equivalence (pinned from Rust + Python) ──

  def test_two_member_bytes_match_rust
    out = PE.build(two_member_input)
    rust_bytes = [RUST_FIXTURE_BYTES_HEX].pack("H*")
    if out.bytes != rust_bytes
      # Find first divergence for diagnostics
      ours = out.bytes.bytes
      theirs = rust_bytes.bytes
      idx = (0...[ours.length, theirs.length].min).find { |i| ours[i] != theirs[i] }
      msg = if idx
        "cross-kit byte divergence at byte #{idx}: " \
        "ruby=0x#{format('%02x', ours[idx])} rust=0x#{format('%02x', theirs[idx])}\n" \
        "ruby hex: #{out.bytes.unpack1('H*')}\n" \
        "rust hex: #{rust_bytes.unpack1('H*')}"
      else
        "cross-kit length mismatch: ruby=#{ours.length}, rust=#{theirs.length}"
      end
      flunk msg
    end
    assert_equal rust_bytes, out.bytes
  end

  def test_two_member_cid_matches_rust
    out = PE.build(two_member_input)
    assert_equal RUST_FIXTURE_CID, out.cid
  end

  def test_rust_bytes_verify_with_foundation_key
    rust_bytes = [RUST_FIXTURE_BYTES_HEX].pack("H*")
    assert PE.verify(rust_bytes, RUST_FIXTURE_CID, FOUNDATION_PUBKEY)
  end
end
