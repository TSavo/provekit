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

    Sort = Struct.new(:name) do
      def self.Int    = new("Int")
      def self.Real   = new("Real")
      def self.String = new("String")
      def self.Bool   = new("Bool")
      def self.Ref    = new("Ref")
      def self.Node   = new("Node")
    end

    # ── Term ────────────────────────────────────────────────────

    def self.var(name:)
      { kind: :var, name: name }
    end

    def self.num(value)
      { kind: :const, value: value.to_i, sort: Sort.Int }
    end

    def self.str(value)
      { kind: :const, value: value.to_s, sort: Sort.String }
    end

    def self.bool(value)
      { kind: :const, value: !!value, sort: Sort.Bool }
    end

    def self.null_ref
      { kind: :const, value: nil, sort: Sort.Ref }
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
        %({"kind":"primitive","name":#{emit_string(sort.name)}})
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

    # ── Declaration marshaling ──────────────────────────────────

    def self.marshal_declarations(decls)
      arr = decls.map do |d|
        obj = {
          kind: "contract",
          name: d.name,
          outBinding: d.out_binding,
        }
        obj[:pre]  = d.pre  if d.pre
        obj[:post] = d.post if d.post
        obj[:inv]  = d.inv  if d.inv
        obj
      end
      Jcs.encode(arr)
    end
  end
end
