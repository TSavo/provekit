# SPDX-License-Identifier: Apache-2.0
#
# Provekit::SelfContracts
#
# Side A orchestrator for the Ruby kit. Walks the canonical Ruby slab,
# mints each contract as a signed layered memento under the foundation
# key, bundles them into a `.proof` envelope whose filename IS its CID.
#
# Mirrors the rust/cpp/go/ts Side A pattern. The contractSetCid we
# produce is the BLAKE3-512 of the JCS encoding of the sorted list of
# signer-independent contractCids; identical inputs produce byte-
# identical CIDs across kits with the same authoring choices.
#
# Run modes:
#   * Direct CLI invocation:   ruby -Ilib lib/provekit/self_contracts.rb [outDir]
#   * Lift-protocol RPC mode:  ruby -Ilib lib/provekit/self_contracts.rb --rpc
#
# References:
#   * implementations/cpp/provekit-self-contracts/mint_cpp_self_contracts.cpp
#   * implementations/rust/provekit-self-contracts/src/bin/mint-self-contracts.rs
#   * protocol/specs/2026-04-30-lift-plugin-protocol.md
#   * protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md (contractCid)
#   * protocol/specs/2026-05-03-contract-set-extension.md         (contractSetCid)

require "base64"
require "fileutils"
require "json"
require "tmpdir"

require_relative "blake3"
require_relative "ir"
require_relative "cbor"
require_relative "signing"
require_relative "proof_envelope"

module Provekit
  module SelfContracts
    PRODUCED_BY  = "provekit-ruby-self-contracts@1.0".freeze
    DECLARED_AT  = "2026-04-30T18:00:00.000Z".freeze
    CATALOG_NAME = "@provekit/ruby-self-contracts".freeze
    CATALOG_VERSION = "1.0.0".freeze
    LAYERED_SCHEMA_VERSION = "2".freeze

    # ---- Slab authoring ---------------------------------------------------
    #
    # A "slab" is the set of contracts authored against one Ruby module.
    # Each slab returns an array of Provekit::IR::ContractDecl values.
    # Names MUST be unique across slabs. Predicates use the kit's IR
    # primitives (already JCS-conformant per test_jcs_conformance.rb).
    #
    # Predicate style mirrors the cpp/go canonical slabs: shape-level
    # claims (length, determinism) about the public API. Z3 cannot
    # discharge most of these in isolation; the value is the living-doc
    # IR shape the verifier indexes against.

    IR_M = Provekit::IR

    def self.slab_blake3
      h_b = ->(s) { IR_M.ctor("Blake3.bytes", s) }
      h_x = ->(s) { IR_M.ctor("Blake3.hex", s) }
      [
        IR_M::ContractDecl.new(
          name: "ruby_blake3_bytes_length_eq_64",
          out_binding: "out",
          post: IR_M.eq(IR_M.ctor("byte_length", h_b.(IR_M.var(name: "s"))), IR_M.num(64)),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_blake3_hex_length_eq_139",
          out_binding: "out",
          post: IR_M.eq(IR_M.ctor("string_length", h_x.(IR_M.var(name: "s"))), IR_M.num(139)),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_blake3_hex_is_deterministic",
          out_binding: "out",
          post: IR_M.eq(h_x.(IR_M.var(name: "s")), h_x.(IR_M.var(name: "s"))),
        ),
      ]
    end

    def self.slab_jcs
      enc = ->(v) { IR_M.ctor("Jcs.encode", v) }
      [
        IR_M::ContractDecl.new(
          name: "ruby_jcs_encode_is_deterministic",
          out_binding: "out",
          post: IR_M.eq(enc.(IR_M.var(name: "v")), enc.(IR_M.var(name: "v"))),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_jcs_encode_true_length_eq_4",
          out_binding: "out",
          post: IR_M.eq(
            IR_M.ctor("string_length", enc.(IR_M.bool(true))),
            IR_M.num(4),
          ),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_jcs_encode_null_length_eq_4",
          out_binding: "out",
          post: IR_M.eq(
            IR_M.ctor("string_length", enc.(IR_M.null_ref)),
            IR_M.num(4),
          ),
        ),
      ]
    end

    def self.slab_cbor
      [
        IR_M::ContractDecl.new(
          name: "ruby_cbor_tstr_roundtrip_length_gte_1",
          out_binding: "out",
          post: IR_M.gte(
            IR_M.ctor("byte_length", IR_M.ctor("Cbor.encode_tstr", IR_M.var(name: "s"))),
            IR_M.num(1),
          ),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_cbor_bstr_roundtrip_length_gte_1",
          out_binding: "out",
          post: IR_M.gte(
            IR_M.ctor("byte_length", IR_M.ctor("Cbor.encode_bstr", IR_M.var(name: "b"))),
            IR_M.num(1),
          ),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_cbor_encode_key_is_deterministic",
          out_binding: "out",
          post: IR_M.eq(
            IR_M.ctor("Cbor.encode_key", IR_M.var(name: "k")),
            IR_M.ctor("Cbor.encode_key", IR_M.var(name: "k")),
          ),
        ),
      ]
    end

    def self.slab_signing
      [
        IR_M::ContractDecl.new(
          name: "ruby_ed25519_signature_length_eq_64",
          out_binding: "out",
          post: IR_M.eq(
            IR_M.ctor("byte_length",
              IR_M.ctor("Signing.sign_with_seed",
                IR_M.var(name: "seed"),
                IR_M.var(name: "msg"))),
            IR_M.num(64),
          ),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_ed25519_pubkey_bytes_length_eq_32",
          out_binding: "out",
          post: IR_M.eq(
            IR_M.ctor("byte_length",
              IR_M.ctor("Signing.pubkey_bytes", IR_M.var(name: "seed"))),
            IR_M.num(32),
          ),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_ed25519_sign_is_deterministic",
          out_binding: "out",
          post: IR_M.eq(
            IR_M.ctor("Signing.sign_with_seed",
              IR_M.var(name: "seed"),
              IR_M.var(name: "msg")),
            IR_M.ctor("Signing.sign_with_seed",
              IR_M.var(name: "seed"),
              IR_M.var(name: "msg")),
          ),
        ),
      ]
    end

    def self.slab_proof_envelope
      [
        IR_M::ContractDecl.new(
          name: "ruby_proof_envelope_cid_has_blake3_512_prefix",
          out_binding: "out",
          post: IR_M.gte(
            IR_M.ctor("string_length",
              IR_M.ctor("ProofEnvelope.build_cid", IR_M.var(name: "input"))),
            IR_M.num(11),
          ),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_proof_envelope_cid_total_length_eq_139",
          out_binding: "out",
          post: IR_M.eq(
            IR_M.ctor("string_length",
              IR_M.ctor("ProofEnvelope.build_cid", IR_M.var(name: "input"))),
            IR_M.num(139),
          ),
        ),
        IR_M::ContractDecl.new(
          name: "ruby_proof_envelope_build_is_deterministic",
          out_binding: "out",
          post: IR_M.eq(
            IR_M.ctor("ProofEnvelope.build_cid", IR_M.var(name: "input")),
            IR_M.ctor("ProofEnvelope.build_cid", IR_M.var(name: "input")),
          ),
        ),
      ]
    end

    # Returns [{label:, path:, contracts:[ContractDecl]}, ...].
    def self.author_all_invariants
      [
        { label: "blake3",         path: "implementations/ruby/lib/provekit/blake3.rb",         contracts: slab_blake3 },
        { label: "jcs",            path: "implementations/ruby/lib/provekit/ir.rb",             contracts: slab_jcs },
        { label: "cbor",           path: "implementations/ruby/lib/provekit/cbor.rb",           contracts: slab_cbor },
        { label: "signing",        path: "implementations/ruby/lib/provekit/signing.rb",        contracts: slab_signing },
        { label: "proof_envelope", path: "implementations/ruby/lib/provekit/proof_envelope.rb", contracts: slab_proof_envelope },
      ]
    end

    # ---- Contract / contract-set CID --------------------------------------
    #
    # contractCid is the signer-independent BLAKE3-512(JCS({name, outBinding,
    # pre?, post?, inv?})). The contractSetCid is BLAKE3-512(JCS(<sorted
    # contractCids>)). Both shapes are byte-identical to the rust/cpp/go/ts
    # peers per spec 2026-05-03-contract-cid-vs-attestation-cid.md and
    # 2026-05-03-contract-set-extension.md.

    def self.contract_cid(decl)
      obj = { "name" => decl.name, "outBinding" => decl.out_binding }
      obj["pre"]  = decl.pre  if decl.pre
      obj["post"] = decl.post if decl.post
      obj["inv"]  = decl.inv  if decl.inv
      jcs = IR_M::Jcs.encode(obj)
      Blake3.hex(jcs)
    end

    def self.compute_contract_set_cid(content_cids)
      sorted = content_cids.dup.sort
      jcs = IR_M::Jcs.encode(sorted)
      Blake3.hex(jcs)
    end

    # ---- Layered memento mint --------------------------------------------
    #
    # Ports rust `provekit-claim-envelope` mint_contract. The result is the
    # JCS-canonical bytes of `{envelope, header, metadata}`; the attestation
    # CID is BLAKE3-512(JCS(envelope)). Per
    # protocol/specs/2026-04-30-memento-envelope-grammar.md.

    def self.hash_value_jcs(value)
      Blake3.hex(IR_M::Jcs.encode(value))
    end

    def self.mint_contract(decl, signer_seed: Provekit::Signing::FOUNDATION_V0_SEED, produced_by: PRODUCED_BY, produced_at: DECLARED_AT, source_path: "")
      raise ArgumentError, "contract must carry at least one of pre/post/inv" if decl.pre.nil? && decl.post.nil? && decl.inv.nil?
      raise ArgumentError, "out_binding must be non-empty" if decl.out_binding.to_s.empty?

      # propertyHash = BLAKE3-512(JCS({pre?, post?, inv?, outBinding}))
      ph = {}
      ph["pre"]  = decl.pre  if decl.pre
      ph["post"] = decl.post if decl.post
      ph["inv"]  = decl.inv  if decl.inv
      ph["outBinding"] = decl.out_binding
      property_hash = hash_value_jcs(ph)

      # bindingHash = BLAKE3-512(JCS({producerId, contractName, propertyHash}))
      bh = {
        "producerId" => produced_by,
        "contractName" => decl.name,
        "propertyHash" => property_hash,
      }
      binding_hash = hash_value_jcs(bh)

      header_cid = contract_cid(decl)

      header = { "schemaVersion" => LAYERED_SCHEMA_VERSION, "kind" => "contract", "cid" => header_cid }
      header["name"] = decl.name
      header["outBinding"] = decl.out_binding
      header["pre"]  = decl.pre  if decl.pre
      header["post"] = decl.post if decl.post
      header["inv"]  = decl.inv  if decl.inv
      header["verdict"] = "holds"
      header["bindingHash"] = binding_hash
      header["propertyHash"] = property_hash
      header["inputCids"] = []

      authoring = {
        "producerKind" => "kit-author",
        "author" => produced_by,
      }
      authoring["note"] = "self-contract from #{source_path}" unless source_path.empty?

      metadata = {
        "authoring" => authoring,
        "producedBy" => produced_by,
        "producedAt" => produced_at,
      }
      metadata["preHash"]  = hash_value_jcs(decl.pre)  if decl.pre
      metadata["postHash"] = hash_value_jcs(decl.post) if decl.post
      metadata["invHash"]  = hash_value_jcs(decl.inv)  if decl.inv

      # Signing message = JCS({header, metadata}); signature is the spec's
      # self-identifying string form ("ed25519:" + base64(64-byte sig)).
      signing_msg = IR_M::Jcs.encode({ "header" => header, "metadata" => metadata })
      signature   = Provekit::Signing.ed25519_sign_string(signer_seed, signing_msg)
      signer_str  = Provekit::Signing.ed25519_pubkey_string(signer_seed)

      envelope = {
        "signer" => signer_str,
        "declaredAt" => produced_at,
        "signature" => signature,
      }
      envelope_jcs = IR_M::Jcs.encode(envelope)
      attestation_cid = Blake3.hex(envelope_jcs)

      memento_jcs = IR_M::Jcs.encode({
        "envelope" => envelope,
        "header" => header,
        "metadata" => metadata,
      })

      {
        cid: attestation_cid,
        contract_cid: header_cid,
        canonical_bytes: memento_jcs.b,
      }
    end

    # ---- Mint orchestrator ------------------------------------------------

    MintResult = Struct.new(
      :cid,
      :contract_set_cid,
      :bytes_len,
      :path,
      :member_count,
      :total_contracts,
      :per_source_counts,
      keyword_init: true,
    )

    # Mints every authored contract as a layered memento, bundles into a
    # `.proof` whose filename IS the catalog CID, writes to
    # `<out_dir>/<catalog-cid>.proof`, and returns a MintResult.
    def self.mint_self_proof(out_dir)
      FileUtils.mkdir_p(out_dir)

      slabs = author_all_invariants
      seed = Provekit::Signing::FOUNDATION_V0_SEED
      signer_pubkey = Provekit::Signing.ed25519_pubkey_string(seed)
      signer_cid = Blake3.hex(signer_pubkey)

      members = {}
      seen_names = {}
      content_cids = []
      per_source_counts = []
      total = 0

      slabs.each do |slab|
        per_source_counts << { label: slab[:label], count: slab[:contracts].length }
        slab[:contracts].each do |decl|
          if seen_names.key?(decl.name)
            raise "duplicate contract name `#{decl.name}` across slabs"
          end
          seen_names[decl.name] = true
          minted = mint_contract(decl, signer_seed: seed, source_path: slab[:path])
          content_cids << minted[:contract_cid]
          members[minted[:cid]] = minted[:canonical_bytes]
          total += 1
        end
      end

      input = Provekit::ProofEnvelope::Input.new(
        name: CATALOG_NAME,
        version: CATALOG_VERSION,
        members: members,
        signer_cid: signer_cid,
        declared_at: DECLARED_AT,
        signer_seed: seed,
      )
      built = Provekit::ProofEnvelope.build(input)

      raise "internal: cid missing blake3-512 prefix: #{built.cid}" unless built.cid.start_with?("blake3-512:")

      cset_cid = compute_contract_set_cid(content_cids)
      out_path = File.join(out_dir, "#{built.cid}.proof")
      File.binwrite(out_path, built.bytes)

      MintResult.new(
        cid: built.cid,
        contract_set_cid: cset_cid,
        bytes_len: built.bytes.bytesize,
        path: out_path,
        member_count: members.length,
        total_contracts: total,
        per_source_counts: per_source_counts,
      )
    end

    # ---- RPC mode (lift-plugin-protocol over NDJSON stdio) ----------------
    #
    # Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
    # Daemon-lifecycle pattern (issue #220 ts Side A): persistent stdio
    # loop, EOF on stdin = graceful shutdown, explicit `shutdown` method
    # acks then exits.

    def self.run_rpc_mode(stdin: $stdin, stdout: $stdout)
      stdin.binmode if stdin.respond_to?(:binmode)
      stdout.sync = true if stdout.respond_to?(:sync=)

      while (line = stdin.gets)
        line = line.strip
        next if line.empty?

        begin
          req = JSON.parse(line)
        rescue JSON::ParserError => e
          write_rpc(stdout, jsonrpc: "2.0", id: nil, error: { code: -32700, message: "Parse error: #{e}" })
          next
        end

        id     = req["id"]
        method = req["method"].to_s

        case method
        when "initialize"
          write_rpc(stdout, jsonrpc: "2.0", id: id, result: {
            name: "ruby-self-contracts",
            version: "1.0.0",
            protocol_version: "provekit-lift/1",
            capabilities: {
              authoring_surfaces: ["ruby-self-contracts"],
              ir_version: "v1.1.0",
              emits_signed_mementos: true,
            },
          })
        when "lift"
          tmp = Dir.mktmpdir("provekit-ruby-rpc-")
          begin
            mint = mint_self_proof(tmp)
            bytes = File.binread(mint.path)
            b64 = Base64.strict_encode64(bytes)
            write_rpc(stdout, jsonrpc: "2.0", id: id, result: {
              kind: "proof-envelope",
              filename_cid: mint.cid,
              contract_set_cid: mint.contract_set_cid,
              bytes_base64: b64,
              diagnostics: [],
            })
          rescue StandardError => e
            write_rpc(stdout, jsonrpc: "2.0", id: id, error: { code: 1005, message: "LIFT_FAILED: #{e.message}" })
          ensure
            FileUtils.remove_entry(tmp) if File.directory?(tmp)
          end
        when "shutdown"
          write_rpc(stdout, jsonrpc: "2.0", id: id, result: nil)
          return 0
        else
          write_rpc(stdout, jsonrpc: "2.0", id: id, error: { code: -32601, message: "METHOD_NOT_FOUND: #{method}" })
        end
      end
      0
    end

    def self.write_rpc(out, payload)
      out.write(JSON.generate(payload))
      out.write("\n")
      out.flush if out.respond_to?(:flush)
    end

    # ---- Direct CLI (also used by smoke tests) ----------------------------

    def self.main(argv)
      if argv.include?("--rpc")
        return run_rpc_mode
      end

      out_dir = argv[0] || File.join(Dir.tmpdir, "provekit-ruby-self-contracts-#{Process.pid}")
      det_dir = File.join(Dir.tmpdir, "provekit-ruby-self-determinism-#{Process.pid}")
      FileUtils.rm_rf(det_dir)

      $stdout.puts "== ProvekIt Ruby self-contracts orchestrator =="
      $stdout.puts ""
      $stdout.puts "output dir: #{out_dir}"

      begin
        mint_a = mint_self_proof(det_dir)
        mint_b = mint_self_proof(out_dir)
      rescue StandardError => e
        $stderr.puts "ERROR: mint failed: #{e.message}"
        $stderr.puts e.backtrace.join("\n") if $DEBUG
        return 1
      end

      $stdout.puts ""
      $stdout.puts "authored:"
      mint_b.per_source_counts.each do |row|
        $stdout.puts format("  %22s  %2d contracts", row[:label], row[:count])
      end
      $stdout.puts format("  %22s  %2d contracts (TOTAL)", "[ALL]", mint_b.total_contracts)

      $stdout.puts ""
      $stdout.puts "minted:"
      $stdout.puts "  .proof file:        #{mint_b.path}"
      $stdout.puts "  bytes:              #{mint_b.bytes_len}"
      $stdout.puts "  members:            #{mint_b.member_count}"
      $stdout.puts "  total contracts:    #{mint_b.total_contracts}"
      $stdout.puts "  catalog CID:        #{mint_b.cid}"
      $stdout.puts "  contractSetCid:     #{mint_b.contract_set_cid}"

      if mint_a.cid != mint_b.cid || mint_a.contract_set_cid != mint_b.contract_set_cid
        $stderr.puts ""
        $stderr.puts "ERROR: byte-determinism check FAILED:"
        $stderr.puts "  run A CID:              #{mint_a.cid}"
        $stderr.puts "  run B CID:              #{mint_b.cid}"
        $stderr.puts "  run A contractSetCid:   #{mint_a.contract_set_cid}"
        $stderr.puts "  run B contractSetCid:   #{mint_b.contract_set_cid}"
        FileUtils.rm_rf(det_dir)
        return 2
      end

      FileUtils.rm_rf(det_dir)
      $stdout.puts "  determinism check:  OK (two runs produced identical CIDs)"
      $stdout.puts ""
      $stdout.puts "== done. Ruby self-application: live. =="
      0
    end
  end
end

# Direct invocation guard
if $PROGRAM_NAME == __FILE__
  exit Provekit::SelfContracts.main(ARGV)
end
