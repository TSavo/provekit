# SPDX-License-Identifier: Apache-2.0
#
# claim_envelope tests: layered-shape construction, byte-equivalence
# against the Rust + Python references, error-path coverage, and the
# contractSetCid conformance test.
#
# Cross-kit byte-equivalence pins are derived from the Rust kit:
#   cargo test -p provekit-claim-envelope --test cross_kit_pin -- --nocapture
#
# The Rust kit is the reference; the Ruby kit must produce byte-identical
# output for the same canonical input. If a pin fails, the mismatch is
# a real divergence -- surface it, don't paper over it.

require "minitest/autorun"
require "json"
require "base64"
require "ed25519"

require_relative "../lib/provekit"

# ---------------------------------------------------------------------------
# Canonical fixtures (match implementations/rust/.../tests/cross_kit_pin.rs)
# ---------------------------------------------------------------------------

# `forall n: Int. n > 0` -- the cross-kit pin's `pre` formula.
PRE_N_GT_0 = {
  "kind" => "forall",
  "name" => "n",
  "sort" => {"kind" => "primitive", "name" => "Int"},
  "body" => {
    "kind" => "atomic",
    "name" => ">",
    "args" => [
      {"kind" => "var", "name" => "n"},
      {"kind" => "const", "value" => 0,
       "sort" => {"kind" => "primitive", "name" => "Int"}},
    ],
  },
}.freeze

# `out = 0` -- the cross-kit pin's `post` formula.
POST_OUT_EQ_0 = {
  "kind" => "atomic",
  "name" => "=",
  "args" => [
    {"kind" => "var", "name" => "out"},
    {"kind" => "const", "value" => 0,
     "sort" => {"kind" => "primitive", "name" => "Int"}},
  ],
}.freeze

def fixture_kwargs
  {
    contract_name: "demo",
    out_binding: "out",
    pre: PRE_N_GT_0,
    post: POST_OUT_EQ_0,
    inv: nil,
    produced_by: "rust-test@1.0",
    produced_at: "2026-04-30T00:00:00.000Z",
    authoring: Provekit::ClaimEnvelope::AuthoringKitAuthor.new(author: "rust-test@1.0"),
    signer_seed: Provekit::Signing::FOUNDATION_V0_SEED,
    input_cids: [],
  }
end

# ---------------------------------------------------------------------------
# Cross-kit byte-equivalence pins (from rust cross_kit_pin.rs / python tests)
# ---------------------------------------------------------------------------

RUST_FIXTURE_BYTES_HEX_FULL =
  "7b22656e76656c6f7065223a7b226465636c617265644174223a22323032362d30342d33" \
  "305430303a30303a30302e3030305a222c227369676e6174757265223a22656432353531" \
  "393a445549436e45753343647a4f76534a49756c5878314e4a794765746b73634f443755" \
  "4857696b3264743733766d47332f375a2b763364673644516d6d6a4c744852556e793369" \
  "454a61657a6c34326a6c584f655543513d3d222c227369676e6572223a22656432353531" \
  "393a49564c34305a7435485352464d6b4c6858793672624c66502b6e747158744d416c35" \
  "594f427069423278493d227d2c22686561646572223a7b2262696e64696e674861736822" \
  "3a22626c616b65332d3531323a3465343962396465626338393963663865333461653931" \
  "343662373234343832313139306662376631376136356534356561346265636465333033" \
  "656337323865643636316132613336303763626432353361633730313264646534363836" \
  "333737633664313966343464393633393232613465376539333034363932366466222c22" \
  "636964223a22626c616b65332d3531323a62636130626239313434623362333565333261" \
  "633039646334333832633838656336363337353031326235613166616663383965363539" \
  "646239363637353637336435363161366436386234643138333138646330326638646437" \
  "663838653665666661336439646262666237653764623137313631306630383731356462" \
  "31222c22696e70757443696473223a5b5d2c226b696e64223a22636f6e7472616374222c" \
  "226e616d65223a2264656d6f222c226f757442696e64696e67223a226f7574222c22706f" \
  "7374223a7b2261726773223a5b7b226b696e64223a22766172222c226e616d65223a226f" \
  "7574227d2c7b226b696e64223a22636f6e7374222c22736f7274223a7b226b696e64223a" \
  "227072696d6974697665222c226e616d65223a22496e74227d2c2276616c7565223a307d" \
  "5d2c226b696e64223a2261746f6d6963222c226e616d65223a223d227d2c22707265223a" \
  "7b22626f6479223a7b2261726773223a5b7b226b696e64223a22766172222c226e616d65" \
  "223a226e227d2c7b226b696e64223a22636f6e7374222c22736f7274223a7b226b696e64" \
  "223a227072696d6974697665222c226e616d65223a22496e74227d2c2276616c7565223a" \
  "307d5d2c226b696e64223a2261746f6d6963222c226e616d65223a223e227d2c226b696e" \
  "64223a22666f72616c6c222c226e616d65223a226e222c22736f7274223a7b226b696e64" \
  "223a227072696d6974697665222c226e616d65223a22496e74227d7d2c2270726f706572" \
  "747948617368223a22626c616b65332d3531323a33636665633638326665646562336666" \
  "656132346339373339653231343262633935323661636238653136326330303431386335" \
  "363466383236663261363735633864616162636235313865366337663131666562363366" \
  "346631336530373634353366643236623833323063336535356466393332383437313535" \
  "65346132222c22736368656d6156657273696f6e223a2232222c2276657264696374223a" \
  "22686f6c6473227d2c226d65746164617461223a7b22617574686f72696e67223a7b2261" \
  "7574686f72223a22727573742d7465737440312e30222c2270726f64756365724b696e64" \
  "223a226b69742d617574686f72227d2c22706f737448617368223a22626c616b65332d35" \
  "31323a363530613835353463316362383831373832393965303637383238376638383939" \
  "366636626633643733646139376238336539383330363264643565653736396338376631" \
  "303830346561333333623237373937343366383837663661656266353235653734643836" \
  "6662636165326461633964323937633235336536376135222c2270726548617368223a22" \
  "626c616b65332d3531323a61663736323664646162346164343233636535653361326335" \
  "633334643065616135346635373132303134376462663765396437363633323032633430" \
  "396539303132383836663730303762616463386439366232396236623836333638646465" \
  "32353461343237666466326632363138346263393834633036616239393232222c227072" \
  "6f64756365644174223a22323032362d30342d33305430303a30303a30302e3030305a22" \
  "2c2270726f64756365644279223a22727573742d7465737440312e30227d7d"

RUST_FIXTURE_CID =
  "blake3-512:b5cd82094dd4d7dab5c73ab8b0f236a031a546335cd0ef1c0d7d70a23ffa3" \
  "6506455b5d5a47e197a61da91fdc64b79901320a6480617ff0c0195526f3523639c"

RUST_FIXTURE_CONTRACT_CID =
  "blake3-512:bca0bb9144b3b35e32ac09dc4382c88ec66375012b5a1fafc89e659db966" \
  "75673d561a6d68b4d18318dc02f8dd7f88e6effa3d9dbbfb7e7db171610f08715db1"

# contractSetCid pin from rust cross_kit_pin.rs: 2-element set
# {contract("demo"), contract("second")}.
RUST_CONTRACT_A_CID =
  "blake3-512:bca0bb9144b3b35e32ac09dc4382c88ec66375012b5a1fafc89e659db966" \
  "75673d561a6d68b4d18318dc02f8dd7f88e6effa3d9dbbfb7e7db171610f08715db1"
RUST_CONTRACT_B_CID =
  "blake3-512:3a59a4b9fd854d194250159d08438539730533371e7b4b1ef71ff458accd" \
  "394bc06e67a88d5ba56f7fe1a6f545843cfeb206ec53404b1820b9766d8d91b00e63"
RUST_CONTRACT_SET_CID =
  "blake3-512:e42f67a1f994723791af102a0427c2563c63a526684a69a03264bc625aee" \
  "b5081381a413ed5ce126800f5eb816d4c922e808cd325450825ef19933d075962506"

# ---------------------------------------------------------------------------

class TestCrossKitByteEquivalence < Minitest::Test
  # Ruby output must be byte-identical to Rust + Python kits for the same input.

  def test_fixture_bytes_match_rust
    out = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    rust_bytes = [RUST_FIXTURE_BYTES_HEX_FULL].pack("H*")
    if out.canonical_bytes != rust_bytes
      limit = [out.canonical_bytes.bytesize, rust_bytes.bytesize].min
      limit.times do |i|
        rb = out.canonical_bytes.bytes[i]
        rb2 = rust_bytes.bytes[i]
        if rb != rb2
          flunk(
            "cross-kit byte divergence at byte #{i}: " \
            "ruby=0x#{rb.to_s(16)} (#{out.canonical_bytes[i].inspect}) " \
            "rust=0x#{rb2.to_s(16)} (#{rust_bytes[i].inspect})\n" \
            "ruby length: #{out.canonical_bytes.bytesize}\n" \
            "rust length: #{rust_bytes.bytesize}\n" \
            "ruby[..120]: #{out.canonical_bytes[0, 120].inspect}\n" \
            "rust[..120]: #{rust_bytes[0, 120].inspect}"
          )
        end
      end
      flunk(
        "cross-kit length mismatch: ruby=#{out.canonical_bytes.bytesize}, " \
        "rust=#{rust_bytes.bytesize}"
      )
    end
    pass
  end

  def test_fixture_attestation_cid_matches_rust
    out = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    assert_equal RUST_FIXTURE_CID, out.cid
  end

  def test_fixture_contract_cid_matches_rust
    out = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    assert_equal RUST_FIXTURE_CONTRACT_CID, out.contract_cid
  end

  def test_contract_cid_function_matches_minted
    # The standalone contract_cid() must equal the minted envelope's
    # contract_cid field, since both compute the same thing.
    kw = fixture_kwargs
    direct = Provekit::ClaimEnvelope.contract_cid(
      contract_name: kw[:contract_name],
      out_binding: kw[:out_binding],
      pre: kw[:pre], post: kw[:post], inv: kw[:inv],
    )
    minted = Provekit::ClaimEnvelope.mint_contract(**kw)
    assert_equal RUST_FIXTURE_CONTRACT_CID, direct
    assert_equal direct, minted.contract_cid
  end

  def test_compute_contract_set_cid_matches_rust
    # Per spec §1: BLAKE3-512(JCS(<sorted contractCids>))
    got = Provekit::ContractSet.compute_cid([RUST_CONTRACT_A_CID, RUST_CONTRACT_B_CID])
    assert_equal RUST_CONTRACT_SET_CID, got
  end

  def test_compute_contract_set_cid_is_order_independent
    a = Provekit::ContractSet.compute_cid([RUST_CONTRACT_A_CID, RUST_CONTRACT_B_CID])
    b = Provekit::ContractSet.compute_cid([RUST_CONTRACT_B_CID, RUST_CONTRACT_A_CID])
    assert_equal a, b
    assert_equal RUST_CONTRACT_SET_CID, a
  end
end

class TestLayeredShape < Minitest::Test
  # Structural conformance with substrate-layers spec §1.

  def parse(env)
    JSON.parse(env.canonical_bytes)
  end

  def test_top_level_has_three_keys
    env = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    assert_equal %w[envelope header metadata].sort, parse(env).keys.sort
  end

  def test_envelope_has_signer_declared_at_signature
    env = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    e = parse(env)["envelope"]
    assert_equal %w[declaredAt signature signer], e.keys.sort
    assert e["signer"].start_with?("ed25519:")
    assert e["signature"].start_with?("ed25519:")
  end

  def test_header_has_required_fields
    env = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    h = parse(env)["header"]
    assert_equal Provekit::ClaimEnvelope::LAYERED_SCHEMA_VERSION, h["schemaVersion"]
    assert_equal "contract", h["kind"]
    assert h["cid"].start_with?("blake3-512:")
    %w[name outBinding verdict bindingHash propertyHash inputCids].each do |k|
      assert h.key?(k), "header.#{k} missing"
    end
  end

  def test_attestation_cid_equals_blake3_of_jcs_envelope
    # Substrate-layers spec §2 R1: envelope CID = hash(JCS(envelope))
    # AFTER signature is embedded.
    env = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    e = parse(env)["envelope"]
    recomputed = Provekit::Blake3.hex(Provekit::IR::Jcs.encode(e))
    assert_equal recomputed, env.cid
  end

  def test_signature_covers_jcs_of_header_metadata
    # Signature is over JCS({"header": header, "metadata": metadata}).
    env = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    parsed = parse(env)
    sig_str = parsed["envelope"]["signature"]
    signer_str = parsed["envelope"]["signer"]
    assert sig_str.start_with?("ed25519:")
    assert signer_str.start_with?("ed25519:")

    sig_bytes = Base64.strict_decode64(sig_str.sub("ed25519:", ""))
    pk_bytes = Base64.strict_decode64(signer_str.sub("ed25519:", ""))
    assert_equal 64, sig_bytes.bytesize
    assert_equal 32, pk_bytes.bytesize

    signing_msg = Provekit::IR::Jcs.encode({
      "header" => parsed["header"],
      "metadata" => parsed["metadata"],
    })

    # Should NOT raise.
    vk = ::Ed25519::VerifyKey.new(pk_bytes)
    vk.verify(sig_bytes, signing_msg)
    pass
  end
end

class TestErrors < Minitest::Test
  def test_empty_contract_rejected
    assert_raises(Provekit::ClaimEnvelope::EmptyContractError) do
      Provekit::ClaimEnvelope.mint_contract(
        contract_name: "x", out_binding: "out",
        pre: nil, post: nil, inv: nil,
        produced_by: "t", produced_at: "2026-04-30T00:00:00.000Z",
        authoring: Provekit::ClaimEnvelope::AuthoringKitAuthor.new(author: "t"),
        signer_seed: Provekit::Signing::FOUNDATION_V0_SEED,
      )
    end
  end

  def test_empty_out_binding_rejected
    assert_raises(Provekit::ClaimEnvelope::EmptyOutBindingError) do
      Provekit::ClaimEnvelope.mint_contract(
        contract_name: "x", out_binding: "",
        pre: PRE_N_GT_0, post: nil, inv: nil,
        produced_by: "t", produced_at: "2026-04-30T00:00:00.000Z",
        authoring: Provekit::ClaimEnvelope::AuthoringKitAuthor.new(author: "t"),
        signer_seed: Provekit::Signing::FOUNDATION_V0_SEED,
      )
    end
  end
end

class TestFromContractDecl < Minitest::Test
  # ClaimEnvelope.from_contract_decl(decl, signer) packages a
  # Provekit::IR::ContractDecl directly.

  def test_from_contract_decl_round_trips
    decl = Provekit::IR::ContractDecl.new(
      name: "demo", out_binding: "out",
      pre: PRE_N_GT_0,
    )
    signer = Provekit::Signing::Signer.foundation_v0(producer_id: "ruby-kit@1.0")
    env = Provekit::ClaimEnvelope.from_contract_decl(
      decl, signer, produced_at: "2026-04-30T00:00:00.000Z",
    )
    assert env.cid.start_with?("blake3-512:")
    assert env.contract_cid.start_with?("blake3-512:")
    parsed = JSON.parse(env.canonical_bytes)
    assert_equal %w[envelope header metadata].sort, parsed.keys.sort
    assert_equal "contract", parsed["header"]["kind"]
    assert_equal "demo", parsed["header"]["name"]
  end

  def test_signer_producer_id_lands_in_metadata
    decl = Provekit::IR::ContractDecl.new(
      name: "demo", out_binding: "out", pre: PRE_N_GT_0,
    )
    signer = Provekit::Signing::Signer.foundation_v0(producer_id: "ruby-kit@1.0")
    env = Provekit::ClaimEnvelope.from_contract_decl(
      decl, signer, produced_at: "2026-04-30T00:00:00.000Z",
    )
    parsed = JSON.parse(env.canonical_bytes)
    assert_equal "ruby-kit@1.0", parsed["metadata"]["producedBy"]
    assert_equal "ruby-kit@1.0", parsed["metadata"]["authoring"]["author"]
    assert_equal "kit-author", parsed["metadata"]["authoring"]["producerKind"]
  end

  def test_signer_sign_claim_delegates_to_from_contract_decl
    # Signer#sign_claim is a convenience wrapper; bytes must be
    # identical to ClaimEnvelope.from_contract_decl(decl, signer).
    decl = Provekit::IR::ContractDecl.new(
      name: "demo", out_binding: "out", pre: PRE_N_GT_0,
    )
    signer = Provekit::Signing::Signer.foundation_v0(producer_id: "ruby-kit@1.0")
    a = signer.sign_claim(decl, produced_at: "2026-04-30T00:00:00.000Z")
    b = Provekit::ClaimEnvelope.from_contract_decl(
      decl, signer, produced_at: "2026-04-30T00:00:00.000Z",
    )
    assert_equal b.canonical_bytes, a.canonical_bytes
    assert_equal b.cid, a.cid
    assert_equal b.contract_cid, a.contract_cid
  end
end

class TestAuthoring < Minitest::Test
  def mint_with_authoring(authoring)
    Provekit::ClaimEnvelope.mint_contract(
      contract_name: "x", out_binding: "out",
      pre: PRE_N_GT_0,
      produced_by: "t", produced_at: "2026-04-30T00:00:00.000Z",
      authoring: authoring,
      signer_seed: Provekit::Signing::FOUNDATION_V0_SEED,
    )
  end

  def test_kit_author_no_note_round_trips
    env = mint_with_authoring(
      Provekit::ClaimEnvelope::AuthoringKitAuthor.new(author: "alice"),
    )
    a = JSON.parse(env.canonical_bytes)["metadata"]["authoring"]
    assert_equal({"producerKind" => "kit-author", "author" => "alice"}, a)
  end

  def test_kit_author_with_note_round_trips
    env = mint_with_authoring(
      Provekit::ClaimEnvelope::AuthoringKitAuthor.new(author: "alice", note: "hand"),
    )
    a = JSON.parse(env.canonical_bytes)["metadata"]["authoring"]
    assert_equal(
      {"producerKind" => "kit-author", "author" => "alice", "note" => "hand"},
      a,
    )
  end

  def test_kit_author_empty_note_treated_as_absent
    env = mint_with_authoring(
      Provekit::ClaimEnvelope::AuthoringKitAuthor.new(author: "alice", note: ""),
    )
    a = JSON.parse(env.canonical_bytes)["metadata"]["authoring"]
    refute a.key?("note")
  end

  def test_lift_round_trips
    env = mint_with_authoring(Provekit::ClaimEnvelope::AuthoringLift.new(
      lifter: "lift-kit@1.0", evidence: "tests",
      source_cid: "blake3-512:source",
    ))
    a = JSON.parse(env.canonical_bytes)["metadata"]["authoring"]
    assert_equal "lift", a["producerKind"]
    assert_equal "lift-kit@1.0", a["lifter"]
    assert_equal "tests", a["evidence"]
    assert_equal "blake3-512:source", a["sourceCid"]
  end

  def test_llm_confidence_truncates_toward_zero
    # Match Rust's `(c * 1000.0) as i64`: truncate, do NOT round.
    # 0.9009 -> 900, not 901.
    env = mint_with_authoring(Provekit::ClaimEnvelope::AuthoringLlm.new(
      llm: "claude", llm_version: "opus-4.7",
      prompt_cid: "blake3-512:p", confidence: 0.9009,
    ))
    a = JSON.parse(env.canonical_bytes)["metadata"]["authoring"]
    assert_equal 900, a["confidence"]
  end
end

class TestDeterminism < Minitest::Test
  def test_same_inputs_same_bytes
    a = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    b = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    assert_equal b.canonical_bytes, a.canonical_bytes
    assert_equal b.cid, a.cid
  end

  def test_changing_pre_changes_cid
    a = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    different_pre = {"kind" => "atomic", "name" => "=", "args" => []}
    b = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs.merge(pre: different_pre))
    refute_equal a.cid, b.cid
  end

  def test_changing_signer_changes_attestation_cid_not_contract_cid
    # Two distinct signers attesting to the same contract produce
    # different attestation CIDs but identical contract CIDs.
    a = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs)
    other_seed = ([0x43] * 32).pack("C*")
    b = Provekit::ClaimEnvelope.mint_contract(**fixture_kwargs.merge(signer_seed: other_seed))
    refute_equal a.cid, b.cid
    assert_equal a.contract_cid, b.contract_cid
  end
end

class TestBridge < Minitest::Test
  def test_mint_bridge_succeeds
    env = Provekit::ClaimEnvelope.mint_bridge(
      produced_by: "t", produced_at: "2026-04-30T00:00:00.000Z",
      source_symbol: "parseInt", source_layer: "ts",
      target_contract_cid: "blake3-512:target", target_layer: "rust",
      ir_arg_sorts: ["String"], ir_return_sort: "Int",
      signer_seed: Provekit::Signing::FOUNDATION_V0_SEED,
    )
    assert env.cid.start_with?("blake3-512:")
    assert_equal "", env.contract_cid
    parsed = JSON.parse(env.canonical_bytes)
    assert_equal "bridge", parsed["header"]["kind"]
    assert_equal "parseInt", parsed["header"]["sourceSymbol"]
  end
end

class TestImplication < Minitest::Test
  def test_mint_implication_succeeds
    env = Provekit::ClaimEnvelope.mint_implication(
      produced_by: "t", produced_at: "2026-04-30T00:00:00.000Z",
      antecedent_hash: "blake3-512:ah", consequent_hash: "blake3-512:ch",
      antecedent_cid: "blake3-512:acid", consequent_cid: "blake3-512:bcid",
      antecedent_slot: "pre", consequent_slot: "post",
      prover: "z3", prover_run_ms: 42,
      signer_seed: Provekit::Signing::FOUNDATION_V0_SEED,
    )
    assert env.cid.start_with?("blake3-512:")
    assert_equal "", env.contract_cid
    parsed = JSON.parse(env.canonical_bytes)
    assert_equal "implication", parsed["header"]["kind"]
    assert_equal(
      ["blake3-512:acid", "blake3-512:bcid"].sort,
      parsed["header"]["inputCids"],
    )
  end
end

class TestContractSetCid < Minitest::Test
  def test_empty_set_is_hash_of_empty_array
    got = Provekit::ContractSet.compute_cid([])
    expected = Provekit::Blake3.hex("[]")
    assert_equal expected, got
  end

  def test_singleton_set
    got = Provekit::ContractSet.compute_cid(["blake3-512:aa"])
    expected = Provekit::Blake3.hex('["blake3-512:aa"]')
    assert_equal expected, got
  end

  def test_duplicates_are_preserved
    # The set CID is a function of the sorted sequence; duplicates in
    # the input affect the output. (The spec treats the input as a
    # sequence-of-cids, not a deduplicated set.)
    a = Provekit::ContractSet.compute_cid(["blake3-512:aa"])
    b = Provekit::ContractSet.compute_cid(["blake3-512:aa", "blake3-512:aa"])
    refute_equal a, b
  end
end
