# SPDX-License-Identifier: Apache-2.0
#
# ProvekIt IR types + JCS canonical JSON emitter for Ruby.
#
# Mirrors the Rust/Go/Java/Python/C++ kits. Uses frozen hashes for
# immutability (Hash with `kind` tag field, similar to Python's approach).

require "json"

module Provekit
  module IR
    # ── Sort ────────────────────────────────────────────────────

    PrimitiveSort = Struct.new(:name) do
      def self.Int    = new("Int")
      def self.Real   = new("Real")
      def self.String = new("String")
      def self.Bool   = new("Bool")
      def self.Ref    = new("Ref")
      def self.Node   = new("Node")
    end

    FunctionSort = Struct.new(:args, :return_) do
    end

    DependentSort = Struct.new(:name, :index_var, :index_sort) do
    end

    RegionSort = Struct.new(:name) do
      def kind = "region"
    end

    Sort = PrimitiveSort

    # ── Term ────────────────────────────────────────────────────

    def self.var(name:)
      { kind: :var, name: name }
    end

    def self.num(value)
      { kind: :const, value: value.to_i, sort: PrimitiveSort.Int }
    end

    def self.str(value)
      { kind: :const, value: value.to_s, sort: PrimitiveSort.String }
    end

    def self.bool(value)
      { kind: :const, value: !!value, sort: PrimitiveSort.Bool }
    end

    def self.null_ref
      { kind: :const, value: nil, sort: PrimitiveSort.Ref }
    end

    def self.ctor(name, *args)
      { kind: :ctor, name: name.to_s, args: args.flatten }
    end

    # ── Formula ─────────────────────────────────────────────────

    def self.atomic(name, *args)
      { kind: :atomic, name: name.to_s, args: args.flatten }
    end

    def self.connective(kind, *operands)
      { kind: kind.to_s, operands: operands.flatten }
    end

    def self.eq(a, b)     = atomic("=", a, b)
    def self.neq(a, b)    = atomic("≠", a, b)
    def self.gt(a, b)     = atomic(">", a, b)
    def self.gte(a, b)    = atomic("≥", a, b)
    def self.lt(a, b)     = atomic("<", a, b)
    def self.lte(a, b)    = atomic("≤", a, b)
    def self.and(*ops)    = connective(:and, *ops)
    def self.or_(*ops)    = connective(:or, *ops)
    def self.implies(a, b) = connective(:implies, a, b)
    def self.not_(a)      = connective(:not, a)
    def self.forall(name:, sort:, body:)
      { kind: "forall", name: name.to_s, sort: sort, body: body }
    end

    # ── Declaration ─────────────────────────────────────────────

    ContractDecl = Struct.new(:name, :out_binding, :pre, :post, :inv, keyword_init: true) do
      def initialize(name:, out_binding: "out", pre: nil, post: nil, inv: nil)
        super
      end
    end

    # Call-edge memento per spec #114 R1.
    # Emitted for each FFI call site where the calling function has a known
    # contract. When targeting a cross-kit symbol, target_contract_cid is nil
    # and target_symbol is populated with "<kit>:<native-name>" for linker
    # resolution per R3.
    CallEdgeDecl = Struct.new(
      :source_contract_cid,
      :target_contract_cid,
      :target_symbol,
      :call_site_file,
      :call_site_line,
      :call_site_column,
      :evidence_term,
      keyword_init: true,
    )

    # Bridge declaration. Locked spec key order:
    #   kind, name, sourceSymbol, sourceLayer, sourceContractCid,
    #   targetContractCid, targetProofCid, targetLayer, notes (optional).
    # `notes` is OMITTED entirely when nil: never emitted as null.
    # Ruby Struct fields are snake_case; the JSON keys are camelCase
    # and produced by the marshaler, not by `to_h`.
    Bridge = Struct.new(
      :name,
      :source_symbol,
      :source_layer,
      :source_contract_cid,
      :target_contract_cid,
      :target_proof_cid,
      :target_layer,
      :notes,
      keyword_init: true,
    ) do
      def initialize(name:, source_symbol:, source_layer:,
                     source_contract_cid:, target_contract_cid:,
                     target_proof_cid:, target_layer:, notes: nil)
        super
      end
    end

    # ── JCS canonical JSON emitter ──────────────────────────────

    module Jcs
      def self.encode(value)
        case value
        when Symbol
          # Symbol values in IR → string (e.g., :atomic → "atomic")
          emit_string(value.to_s)
        when nil
          emit_const(nil)
        when Integer
          emit_const(value)
        when true, false
          emit_const(value)
        when String
          emit_string(value)
        when Array
          emit_array(value)
        when Hash
          emit_object(value)
        when Sort
          emit_sort(value)
        when Struct
          value.respond_to?(:to_h) ? emit_object(value.to_h) : raise("unsupported struct: #{value.class}")
        else
          raise "unsupported JCS value: #{value.class}"
        end
      end

      def self.emit_object(hash)
        # Sort keys alphabetically (convert Symbol to String for sorting)
        keys = hash.keys.sort_by { |k| k.to_s }
        pairs = keys.map do |k|
          v = hash[k]
          "#{emit_string(k.to_s)}:#{encode(v)}"
        end
        "{#{pairs.join(',')}}"
      end

      def self.emit_array(arr)
        "[#{arr.map { |v| encode(v) }.join(',')}]"
      end

      def self.emit_string(str)
        # JCS-compatible escaping: " \ and control chars as \u00XX (lowercase)
        escaped = str.to_s.gsub('\\', '\\\\\\\\').gsub('"', '\\"')
        escaped = escaped.chars.map do |c|
          if c.ord < 0x20
            "\\u00#{sprintf('%02x', c.ord)}"
          else
            c
          end
        end.join
        "\"#{escaped}\""
      rescue
        # Fallback for binary strings
        "\"#{str.to_s.gsub('"', '\\"').gsub('\\', '\\\\\\\\')}\""
      end

      def self.emit_sort(sort)
        case sort
        when PrimitiveSort
          %({"kind":"primitive","name":#{emit_string(sort.name)}})
        when FunctionSort
          args_json = "[#{sort.args.map { |a| encode(a) }.join(',')}]"
          %({"kind":"function","args":#{args_json},"return":#{encode(sort.return_)}})
        when DependentSort
          %({"kind":"dependent","name":#{emit_string(sort.name)},"indexVar":#{emit_string(sort.index_var)},"indexSort":#{emit_sort(sort.index_sort)}})
        when RegionSort
          %({"kind":"region","name":#{emit_string(sort.name)}})
        else
          raise "unknown sort: #{sort.class}"
        end
      end

      def self.emit_const(value)
        case value
        when nil    then "null"
        when true   then "true"
        when false  then "false"
        when Integer then value.to_s
        when String then emit_string(value)
        else value.to_s
        end
      end
    end

    # ── Call-edge marshaling ──────────────────────────────────

    # Serialize a list of CallEdgeDecl to JCS JSON (sorted by file/line/col
    # for byte-deterministic output per spec #114 R5).
    def self.marshal_call_edges(edges)
      sorted = edges.sort_by do |e|
        [e.call_site_file.to_s, e.call_site_line.to_i, e.call_site_column.to_i]
      end
      arr = sorted.map do |e|
        obj = {
          callSiteLocus:  {
            column: e.call_site_column,
            file:   e.call_site_file,
            line:   e.call_site_line,
          },
          evidenceTerm:   e.evidence_term,
          kind:           "call-edge",
          schemaVersion:  "1",
          sourceContractCid: e.source_contract_cid,
          targetSymbol:   e.target_symbol,
        }
        # targetContractCid is omitted when nil (cross-kit call)
        obj[:targetContractCid] = e.target_contract_cid unless e.target_contract_cid.nil?
        obj
      end
      Jcs.encode(arr)
    end

    # ── Declaration marshaling ──────────────────────────────────

    def self.marshal_declarations(decls)
      arr = decls.map do |d|
        case d
        when Bridge
          obj = {
            kind: "bridge",
            name: d.name,
            sourceSymbol: d.source_symbol,
            sourceLayer: d.source_layer,
            sourceContractCid: d.source_contract_cid,
            targetContractCid: d.target_contract_cid,
            targetProofCid: d.target_proof_cid,
            targetLayer: d.target_layer,
          }
          # `notes` is omitted entirely when nil; never emitted as null.
          # This is the byte-equality rule that keeps every kit in sync
          # (spec 2026-04-30-ir-formal-grammar.md §BridgeDeclaration).
          obj[:notes] = d.notes if d.notes
          obj
        when ContractDecl
          obj = {
            kind: "contract",
            name: d.name,
            outBinding: d.out_binding,
          }
          obj[:pre]  = d.pre  if d.pre
          obj[:post] = d.post if d.post
          obj[:inv]  = d.inv  if d.inv
          obj
        else
          raise "unknown declaration type: #{d.class}"
        end
      end
      Jcs.encode(arr)
    end
  end
end
