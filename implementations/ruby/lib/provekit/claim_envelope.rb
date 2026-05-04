# SPDX-License-Identifier: Apache-2.0
#
# Provekit::ClaimEnvelope -- claim-envelope minter (v1.2 layered shape).
#
# `mint_contract` / `mint_bridge` / `mint_implication` build a signed
# memento in the v1.2 LAYERED shape introduced by
# `protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md`:
#
#   { "envelope": {...}, "header": {...}, "metadata": {...} }
#
#   * envelope = { signer, declaredAt, signature }
#       The signature is computed over JCS({"header": header,
#       "metadata": metadata}). The envelope's CID (= attestation CID)
#       is BLAKE3-512(JCS(envelope)) AFTER the signature has been
#       embedded.
#
#   * header   = substrate-load-bearing data the verifier reads:
#                schemaVersion, kind, cid, plus kind-specific REQUIRED
#                fields and the derived hashes (bindingHash,
#                propertyHash, verdict, inputCids) used by the
#                resolve/index pipeline.
#
#   * metadata = everything else (authoring attribution, lifecycle
#                strings like producedBy/producedAt, derived per-formula
#                hashes that are pure tooling convenience). Opaque to
#                the substrate verifier; signed transitively via the
#                envelope.
#
# Reference (authoritative):
#   implementations/rust/provekit-claim-envelope/src/lib.rs
#   implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/claim_envelope.py
#
# Cross-kit byte-equivalence:
#   The Ruby output is byte-identical to the Rust + Python kits for the
#   same canonical input. See test/test_claim_envelope.rb.

require_relative "blake3"
require_relative "ir"
require_relative "signing"

module Provekit
  module ClaimEnvelope
    # The layered-shape schema version stamped into every memento header
    # emitted by this kit. Older flat mementos carry "1"; verifiers
    # branch on this string at load time.
    LAYERED_SCHEMA_VERSION = "2".freeze

    # ---- Errors ----------------------------------------------------------

    class Error < StandardError; end
    class EmptyContractError < Error; end
    class EmptyOutBindingError < Error; end

    # ---- Authoring (typed union mirrored from rust/python kits) ----------

    # Hand-authored by a human or kit, no inference.
    # `note` is optional; nil or empty string is treated as absent.
    AuthoringKitAuthor = Struct.new(:author, :note, keyword_init: true) do
      def initialize(author:, note: nil)
        super
      end
    end

    # Produced by a structural lifter from source.
    # `evidence` is the lifter's classification ("tests", "types", ...).
    # `source_cid` is optional; nil or empty string is treated as absent.
    AuthoringLift = Struct.new(:lifter, :evidence, :source_cid, keyword_init: true) do
      def initialize(lifter:, evidence:, source_cid: nil)
        super
      end
    end

    # Produced by an LLM, with prompt provenance.
    # `confidence` is a Float in [0, 1]; serialized as `(confidence *
    # 1000).to_i` (truncating toward zero for non-negative values; this
    # matches Rust's `(c * 1000.0) as i64` cast which truncates toward
    # zero, NOT rounding).
    # `rationale` is optional; nil or empty string is treated as absent.
    AuthoringLlm = Struct.new(
      :llm, :llm_version, :prompt_cid, :confidence, :rationale,
      keyword_init: true,
    ) do
      def initialize(llm:, llm_version:, prompt_cid:, confidence:, rationale: nil)
        super
      end
    end

    # ---- Result type -----------------------------------------------------

    # Result of minting a layered claim envelope. Three fields:
    # * canonical_bytes: JCS-canonical bytes of the full layered memento
    #   `{envelope, header, metadata}`. Binary String (ASCII-8BIT).
    # * cid: the attestation CID = BLAKE3-512(JCS(envelope)) AFTER the
    #   signature has been embedded. Self-identifying string form
    #   "blake3-512:<128 hex>".
    # * contract_cid: signer-independent content CID for contract
    #   mementos. Empty string for bridges and implications.
    Minted = Struct.new(:canonical_bytes, :cid, :contract_cid, keyword_init: true)

    # ---- Public surface --------------------------------------------------

    # Build a v1.2-layered ClaimEnvelope from a Provekit::IR::ContractDecl.
    #
    # Required:
    #   decl        -- ContractDecl with name, out_binding, and at least
    #                  one of pre/post/inv populated.
    #   signer      -- Provekit::Signing::Signer (Ed25519 seed + producer-id).
    #   produced_at -- ISO-8601 timestamp (ms precision, trailing 'Z').
    #
    # Optional:
    #   authoring   -- defaults to AuthoringKitAuthor.new(author: signer.producer_id).
    #   input_cids  -- defaults to []; sorted lex inside the header.
    def self.from_contract_decl(decl, signer, produced_at:, authoring: nil, input_cids: nil)
      authoring ||= AuthoringKitAuthor.new(author: signer.producer_id)
      mint_contract(
        contract_name: decl.name,
        out_binding: decl.out_binding,
        pre: decl.pre,
        post: decl.post,
        inv: decl.inv,
        produced_by: signer.producer_id,
        produced_at: produced_at,
        authoring: authoring,
        signer_seed: signer.seed,
        input_cids: input_cids || [],
      )
    end

    # Compute the **content** CID of a contract (signer-independent).
    #
    # Per `protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md`
    # §1, this is the BLAKE3-512 of the JCS encoding of the contract's
    # substrate-load-bearing fields: name, outBinding, and any of
    # pre/post/inv that are present. Two distinct signers attesting to
    # the same logical contract produce the same contractCid.
    def self.contract_cid(contract_name:, out_binding:, pre: nil, post: nil, inv: nil)
      obj = {
        "name" => contract_name,
        "outBinding" => out_binding,
      }
      obj["pre"]  = pre  unless pre.nil?
      obj["post"] = post unless post.nil?
      obj["inv"]  = inv  unless inv.nil?
      hash_value(obj)
    end

    # Mint a v1.2-layered contract claim envelope.
    # Byte-identical to Rust's `mint_contract` for the same canonical
    # inputs (see test/test_claim_envelope.rb cross-kit tests).
    def self.mint_contract(contract_name:, out_binding:, produced_by:, produced_at:,
                           authoring:, signer_seed:,
                           pre: nil, post: nil, inv: nil, input_cids: nil)
      if pre.nil? && post.nil? && inv.nil?
        raise EmptyContractError, "mint_contract: at least one of pre/post/inv must be present"
      end
      if out_binding.to_s.empty?
        raise EmptyOutBindingError, "mint_contract: outBinding must not be empty"
      end

      # DERIVED:
      #   propertyHash = hash(JCS({pre?, post?, inv?, outBinding}))
      #   bindingHash  = hash(JCS({producerId, contractName, propertyHash}))
      ph = {}
      ph["pre"]  = pre  unless pre.nil?
      ph["post"] = post unless post.nil?
      ph["inv"]  = inv  unless inv.nil?
      ph["outBinding"] = out_binding
      property_hash = hash_value(ph)

      binding_hash = hash_value({
        "producerId" => produced_by,
        "contractName" => contract_name,
        "propertyHash" => property_hash,
      })

      header_cid = contract_cid(
        contract_name: contract_name,
        out_binding: out_binding,
        pre: pre, post: post, inv: inv,
      )

      header = {
        "schemaVersion" => LAYERED_SCHEMA_VERSION,
        "kind" => "contract",
        "cid" => header_cid,
        "name" => contract_name,
        "outBinding" => out_binding,
      }
      header["pre"]  = pre  unless pre.nil?
      header["post"] = post unless post.nil?
      header["inv"]  = inv  unless inv.nil?
      header["verdict"] = "holds"
      header["bindingHash"] = binding_hash
      header["propertyHash"] = property_hash
      sorted_inputs = (input_cids || []).map(&:to_s).sort
      header["inputCids"] = sorted_inputs

      metadata = {
        "authoring" => authoring_to_value(authoring),
        "producedBy" => produced_by,
        "producedAt" => produced_at,
      }
      metadata["preHash"]  = hash_value(pre)  unless pre.nil?
      metadata["postHash"] = hash_value(post) unless post.nil?
      metadata["invHash"]  = hash_value(inv)  unless inv.nil?

      assemble_layered(header, metadata, produced_at, signer_seed, header_cid)
    end

    # Mint a v1.2-layered bridge claim envelope.
    # Byte-identical to Rust's `mint_bridge` for the same canonical inputs.
    #
    # DERIVED:
    #   bindingHash  = hash(JCS({sourceLayer, sourceSymbol}))
    #   propertyHash = hash("bridge:" + sourceSymbol)
    #   inputCids    = [target_contract_cid]
    def self.mint_bridge(produced_by:, produced_at:, source_symbol:, source_layer:,
                         target_contract_cid:, target_layer:, ir_arg_sorts:, ir_return_sort:,
                         signer_seed:, notes: "")
      arg_sorts = ir_arg_sorts.map(&:to_s)

      binding_hash = hash_value({
        "sourceLayer" => source_layer,
        "sourceSymbol" => source_symbol,
      })
      property_hash = hash_string("bridge:#{source_symbol}")

      header_cid = hash_value({
        "sourceSymbol" => source_symbol,
        "sourceLayer" => source_layer,
        "targetContractCid" => target_contract_cid,
        "targetLayer" => target_layer,
        "irArgSorts" => arg_sorts,
        "irReturnSort" => ir_return_sort,
      })

      header = {
        "schemaVersion" => LAYERED_SCHEMA_VERSION,
        "kind" => "bridge",
        "cid" => header_cid,
        "sourceSymbol" => source_symbol,
        "sourceLayer" => source_layer,
        "targetContractCid" => target_contract_cid,
        "targetLayer" => target_layer,
        "irArgSorts" => arg_sorts,
        "irReturnSort" => ir_return_sort,
        "verdict" => "holds",
        "bindingHash" => binding_hash,
        "propertyHash" => property_hash,
        "inputCids" => [target_contract_cid],
      }

      metadata = {
        "producedBy" => produced_by,
        "producedAt" => produced_at,
      }
      metadata["notes"] = notes unless notes.to_s.empty?

      assemble_layered(header, metadata, produced_at, signer_seed, "")
    end

    # Mint a v1.2-layered implication claim envelope.
    # Byte-identical to Rust's `mint_implication` for the same canonical
    # inputs.
    #
    # DERIVED:
    #   bindingHash  = hash(JCS({antecedentHash, consequentHash}))
    #   propertyHash = hash("implication:" + ah + ":" + ch)
    #   inputCids    = sorted([antecedent_cid, consequent_cid])
    def self.mint_implication(produced_by:, produced_at:, antecedent_hash:, consequent_hash:,
                              antecedent_cid:, consequent_cid:, antecedent_slot:, consequent_slot:,
                              prover:, prover_run_ms:, signer_seed:,
                              smt_lib_input: "", proof_witness: "")
      binding_hash = hash_value({
        "antecedentHash" => antecedent_hash,
        "consequentHash" => consequent_hash,
      })
      property_hash = hash_string("implication:#{antecedent_hash}:#{consequent_hash}")

      header_cid = hash_value({
        "antecedentHash" => antecedent_hash,
        "consequentHash" => consequent_hash,
        "antecedentCid" => antecedent_cid,
        "consequentCid" => consequent_cid,
        "antecedentSlot" => antecedent_slot,
        "consequentSlot" => consequent_slot,
      })

      input_cids_sorted = [antecedent_cid, consequent_cid].sort

      header = {
        "schemaVersion" => LAYERED_SCHEMA_VERSION,
        "kind" => "implication",
        "cid" => header_cid,
        "antecedentHash" => antecedent_hash,
        "consequentHash" => consequent_hash,
        "antecedentCid" => antecedent_cid,
        "consequentCid" => consequent_cid,
        "antecedentSlot" => antecedent_slot,
        "consequentSlot" => consequent_slot,
        "verdict" => "holds",
        "bindingHash" => binding_hash,
        "propertyHash" => property_hash,
        "inputCids" => input_cids_sorted,
      }

      metadata = {
        "producedBy" => produced_by,
        "producedAt" => produced_at,
        "prover" => prover,
        "proverRunMs" => prover_run_ms.to_i,
      }
      metadata["smtLibInput"]  = smt_lib_input  unless smt_lib_input.to_s.empty?
      metadata["proofWitness"] = proof_witness  unless proof_witness.to_s.empty?

      assemble_layered(header, metadata, produced_at, signer_seed, "")
    end

    # ---- Internals (mirror rust lib.rs / python claim_envelope.py) -------

    # BLAKE3-512(JCS(value)) -- "blake3-512:<128 hex>".
    def self.hash_value(value)
      Provekit::Blake3.hex(Provekit::IR::Jcs.encode(value))
    end

    # BLAKE3-512(s) -- s treated as UTF-8 bytes.
    def self.hash_string(s)
      Provekit::Blake3.hex(s.to_s)
    end

    # JCS-canonical bytes of `{"header": header, "metadata": metadata}`.
    # This is the message the envelope's Ed25519 signature covers
    # (substrate-layers spec §2 R2).
    def self.signing_bytes(header, metadata)
      Provekit::IR::Jcs.encode({"header" => header, "metadata" => metadata})
    end

    # Lower an Authoring block to a JCS-friendly Hash.
    # Mirrors `authoring_to_value` in rust kit / `_authoring_to_value` in
    # python kit. Empty notes/source_cid/rationale are treated as absent.
    def self.authoring_to_value(a)
      case a
      when AuthoringKitAuthor
        h = {"producerKind" => "kit-author", "author" => a.author}
        h["note"] = a.note if a.note && !a.note.empty?
        h
      when AuthoringLift
        h = {
          "producerKind" => "lift",
          "lifter" => a.lifter,
          "evidence" => a.evidence,
        }
        h["sourceCid"] = a.source_cid if a.source_cid && !a.source_cid.empty?
        h
      when AuthoringLlm
        # Rust's `(confidence * 1000.0) as i64` truncates toward zero.
        # Ruby's `Integer(x)` raises; `x.to_i` truncates toward zero
        # for both positive and negative finite floats, matching rust.
        confidence_i = (a.confidence * 1000.0).to_i
        h = {
          "producerKind" => "llm",
          "llm" => a.llm,
          "llmVersion" => a.llm_version,
          "promptCid" => a.prompt_cid,
          "confidence" => confidence_i,
        }
        h["rationale"] = a.rationale if a.rationale && !a.rationale.empty?
        h
      else
        raise ArgumentError, "unknown Authoring variant: #{a.class}"
      end
    end

    # Sign (header, metadata), embed signature into envelope, JCS the
    # full {envelope, header, metadata} triple. Returns Minted.
    #
    # Mirrors `assemble_layered` in rust:
    #   1. signer_str  = ed25519_pubkey_string(seed)
    #   2. signing_msg = JCS({"header": header, "metadata": metadata})
    #   3. signature   = ed25519_sign_string(seed, signing_msg)
    #   4. envelope    = {signer, declaredAt, signature}
    #   5. attestation_cid = BLAKE3-512(JCS(envelope))
    #   6. memento     = {envelope, header, metadata}
    #   7. canonical_bytes = JCS(memento) -> bytes
    def self.assemble_layered(header, metadata, declared_at, signer_seed, content_cid)
      signer_str = Provekit::Signing.ed25519_pubkey_string(signer_seed)
      signing_msg = signing_bytes(header, metadata)
      signature = Provekit::Signing.ed25519_sign_string(signer_seed, signing_msg)

      envelope = {
        "signer" => signer_str,
        "declaredAt" => declared_at,
        "signature" => signature,
      }
      envelope_jcs = Provekit::IR::Jcs.encode(envelope)
      attestation_cid = Provekit::Blake3.hex(envelope_jcs)

      memento_jcs = Provekit::IR::Jcs.encode({
        "envelope" => envelope,
        "header" => header,
        "metadata" => metadata,
      })

      Minted.new(
        canonical_bytes: memento_jcs.b,
        cid: attestation_cid,
        contract_cid: content_cid,
      )
    end
  end
end
