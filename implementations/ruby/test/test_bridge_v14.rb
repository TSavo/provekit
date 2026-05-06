# SPDX-License-Identifier: Apache-2.0
#
# Ruby Bridge IR v1.4 — CID conformance + round-trip tests.
# Pins against the canonical bridge_decl_v1_4 fixture in
# conformance/fixtures.toml. Mirrors rust's
# provekit-claim-envelope/tests/bridge_v14_roundtrip.rs.

require "fileutils"
require "minitest/autorun"
require "tmpdir"

require_relative "../lib/provekit/blake3"
require_relative "../lib/provekit/ir"
require_relative "../lib/provekit/signing"
require_relative "../lib/provekit/claim_envelope"
require_relative "../lib/provekit/bridge_v14"

class TestBridgeV14 < Minitest::Test
  FOUNDATION_SEED = ([0x42] * 32).pack("C*").freeze

  # ── Golden values from conformance/fixtures.toml bridge_decl_v1_4 ──

  DECLARED_AT = "2026-05-03T00:00:00.000Z"
  NAME = "rust-canonical-bridge-fixture"
  SOURCE_SYMBOL = "parseInt"
  SOURCE_LAYER = "rust-kit"
  SOURCE_CONTRACT_CID = "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
  TARGET_CID = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"
  TARGET_LAYER = "rust-kit"
  PRODUCED_BY = "provekit-canonical-reference@v1.4"
  PRODUCED_AT_ALT = "2026-05-03T00:00:00.000Z"

  EXPECTED_HASH = "blake3-512:660ce98742d1f7ff326c994e4f6aba4d396d7fba0914db91a142c489e6d0901a7eff0ca206ce49bfa5b71eda289a138049fa8cf6461c5ef353703a78c0966cf2"

  # ── Test: round-trip produces the pinned protocol CID ──

  def test_round_trip_cid_matches_fixture_pin
    target = Provekit::BridgeV14::BridgeTarget.contract(cid: TARGET_CID)

    args = Provekit::BridgeV14::MintBridgeV14Args.new(
      name: NAME,
      source_symbol: SOURCE_SYMBOL,
      source_layer: SOURCE_LAYER,
      source_contract_cid: SOURCE_CONTRACT_CID,
      target: target,
      target_layer: TARGET_LAYER,
      produced_by: PRODUCED_BY,
      produced_at: PRODUCED_AT_ALT,
      declared_at: DECLARED_AT,
      signer_seed: FOUNDATION_SEED,
    )

    minted = Provekit::ClaimEnvelope.mint_bridge_v14(args)

    jcs_str = minted.canonical_bytes

    actual_hash = Provekit::Blake3.hex(jcs_str)
    assert_equal EXPECTED_HASH, actual_hash, "BLAKE3-512 CID must match pinned fixture"
  end

  # ── Test: tagged union variants ──

  def test_target_contract_variant
    target = Provekit::BridgeV14::BridgeTarget.contract(cid: "blake3-512:abc")
    assert_equal "contract", target.kind
    assert_equal "blake3-512:abc", target.cid
    assert_equal({ "kind" => "contract", "cid" => "blake3-512:abc" }, target.to_h)
  end

  def test_target_contractSet_variant
    target = Provekit::BridgeV14::BridgeTarget.contractSet(cid: "blake3-512:def")
    assert_equal "contractSet", target.kind
    assert_equal({ "kind" => "contractSet", "cid" => "blake3-512:def" }, target.to_h)
  end

  def test_target_rejects_empty_cid
    assert_raises(ArgumentError) { Provekit::BridgeV14::BridgeTarget.contract(cid: "") }
  end

  def test_target_rejects_invalid_kind
    assert_raises(ArgumentError) { Provekit::BridgeV14::BridgeTarget.new(kind: "bad", cid: "x") }
  end

  # ── Test: metadata omission (None fields) ──

  def test_metadata_none_fields_omitted
    metadata = Provekit::BridgeV14::BridgeMetadataV14.new(
      target_layer: "rust-kit",
    )
    h = metadata.to_h
    assert_equal({ "targetLayer" => "rust-kit" }, h)
    refute h.key?("targetWitnessCid")
    refute h.key?("targetBinaryCid")
  end

  def test_metadata_all_fields_emitted
    metadata = Provekit::BridgeV14::BridgeMetadataV14.new(
      target_witness_cid: "blake3-512:wit",
      target_binary_cid: "blake3-512:bin",
      target_layer: "rust",
      target_contract_set_cid: "blake3-512:set",
      produced_by: "test",
      produced_at: "2026-01-01T00:00:00Z",
    )
    h = metadata.to_h
    assert_equal 6, h.size
    assert_equal "blake3-512:wit", h["targetWitnessCid"]
  end

  # ── Test: header shape matches §1.R3 ──

  def test_header_has_canonical_seven_fields
    target = Provekit::BridgeV14::BridgeTarget.contract(cid: TARGET_CID)
    header = Provekit::BridgeV14::BridgeHeaderV14.new(
      name: "test",
      source_symbol: "foo",
      source_layer: "rust-kit",
      source_contract_cid: SOURCE_CONTRACT_CID,
      target: target,
    ).to_h

    # 7 canonical fields per spec §1.R3
    assert_equal 7, header.size
    assert_equal "1", header["schemaVersion"]
    assert_equal "bridge", header["kind"]
    assert_equal "test", header["name"]
    assert_equal "foo", header["sourceSymbol"]
    assert_equal "rust-kit", header["sourceLayer"]
    assert_equal SOURCE_CONTRACT_CID, header["sourceContractCid"]
    assert_equal target.to_h, header["target"]
  end
end
