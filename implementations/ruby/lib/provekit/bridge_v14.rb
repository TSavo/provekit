# SPDX-License-Identifier: Apache-2.0
#
# Provekit::BridgeV14: v1.4 BridgeDeclaration IR types.
#
# Per protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R1-R6
# and substrate-layers-envelope-header-body.md §1-2.
#
# Canonical reference:
#   implementations/rust/provekit-claim-envelope/src/lib.rs (mint_bridge_v14)

require "json"

module Provekit
  module BridgeV14
    # Schema version stamp on v1.4 layered bridge headers.
    SCHEMA_VERSION = "1"

    # ── Tagged-union target ─────────────────────────

    class BridgeTarget
      attr_reader :kind, :cid

      def initialize(kind:, cid:)
        raise ArgumentError, "kind must be 'contract' or 'contractSet'" unless %w[contract contractSet].include?(kind)
        raise ArgumentError, "cid must be a non-empty string" if cid.to_s.empty?
        @kind = kind
        @cid = cid
      end

      def self.contract(cid:)
        new(kind: "contract", cid: cid)
      end

      def self.contractSet(cid:)
        new(kind: "contractSet", cid: cid)
      end

      def to_h
        { "kind" => kind, "cid" => cid }
      end
    end

    # ── Header (7 fields, substrate-verified) ───────

    class BridgeHeaderV14
      attr_reader :name, :source_symbol, :source_layer, :source_contract_cid, :target

      def initialize(name:, source_symbol:, source_layer:, source_contract_cid:, target:)
        @name = name
        @source_symbol = source_symbol
        @source_layer = source_layer
        @source_contract_cid = source_contract_cid
        @target = target # BridgeTarget
      end

      def to_h
        {
          "schemaVersion" => SCHEMA_VERSION,
          "kind" => "bridge",
          "name" => @name,
          "sourceSymbol" => @source_symbol,
          "sourceLayer" => @source_layer,
          "sourceContractCid" => @source_contract_cid,
          "target" => @target.to_h,
        }
      end
    end

    # ── Metadata (6 optional axes, None = omitted) ───

    class BridgeMetadataV14
      attr_reader :target_witness_cid, :target_binary_cid, :target_layer,
                  :target_contract_set_cid, :produced_by, :produced_at

      def initialize(target_witness_cid: nil, target_binary_cid: nil,
                     target_layer: nil, target_contract_set_cid: nil,
                     produced_by: nil, produced_at: nil)
        @target_witness_cid = target_witness_cid
        @target_binary_cid = target_binary_cid
        @target_layer = target_layer
        @target_contract_set_cid = target_contract_set_cid
        @produced_by = produced_by
        @produced_at = produced_at
      end

      # Only Some fields are emitted; None fields OMITTED per spec §1.R2.
      def to_h
        h = {}
        h["targetWitnessCid"] = @target_witness_cid if @target_witness_cid
        h["targetBinaryCid"] = @target_binary_cid if @target_binary_cid
        h["targetLayer"] = @target_layer if @target_layer
        h["targetContractSetCid"] = @target_contract_set_cid if @target_contract_set_cid
        h["producedBy"] = @produced_by if @produced_by
        h["producedAt"] = @produced_at if @produced_at
        h
      end
    end

    # ── Mint inputs ─────────────────────────────────

    MintBridgeV14Args = Struct.new(
      :name, :source_symbol, :source_layer, :source_contract_cid,
      :target, # BridgeTarget
      :target_witness_cid, :target_binary_cid, :target_layer,
      :target_contract_set_cid, :produced_by, :produced_at,
      :declared_at, :signer_seed,
      keyword_init: true
    )
  end
end
