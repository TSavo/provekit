# SPDX-License-Identifier: Apache-2.0
#
# ProvekIt dry-validation lift adapter.
#
# Walks dry-validation schemas and lifts validation rules to canonical IR.
# dry-validation is the Ruby equivalent of Pydantic (Python), Zod (TypeScript),
# and Java Bean Validation.
#
# Example:
#   UserSchema = Dry::Schema.Params do
#     required(:name).filled(:string, min_size?: 1)
#     required(:age).filled(:integer, gteq?: 0, lteq?: 150)
#     optional(:email).filled(:string, format?: /@/)
#   end
#
#   decls = Provekit::Lift::DryValidation.lift(UserSchema)
#   # => [ContractDecl(name: "User.name", pre: ...), ...]

module Provekit
  module Lift
    module DryValidation
      def self.lift(schema, class_name = "Schema")
        decls = []
        return decls unless schema.respond_to?(:key_map)

        # dry-validation v1: key_map provides field definitions
        key_map = schema.key_map rescue nil
        return decls unless key_map

        key_map.each do |key|
          field = key.name.to_s
          opts = {}

          # Extract type constraint from rule
          if key.filter_schema_dsl.respond_to?(:types)
            types = key.filter_schema_dsl.types rescue []
            type = types.first
            opts[:type] = type&.primitive&.to_s if type&.respond_to?(:primitive)
          end

          # Extract macros from rule (gteq?, lteq?, min_size?, etc.)
          rule = schema.rules[field] rescue nil
          if rule
            # Walk rule AST for predicates
            extract_predicates(rule, opts)
          end

          f = build_formula(field, opts)
          decls << IR::ContractDecl.new(
            name: "#{class_name}.#{field}",
            pre: f
          ) if f
        end

        decls
      rescue => e
        decls
      end

      def self.extract_predicates(rule, opts)
        return unless rule.respond_to?(:ast)

        # dry-validation AST is a nested array of predicates
        ast = rule.ast rescue nil
        return unless ast.is_a?(Array)

        walk_predicates(ast, opts)
      end

      def self.walk_predicates(ast, opts)
        return if ast.empty?

        op = ast[0]
        case op
        when :and, :or
          ast[1..].each { |child| walk_predicates(child, opts) }
        when :key?
          # skip key check - we already know the field name
        when :predicate
          # [:predicate, [:name?, [[:name, "value"], ...]]]
          predicate = ast[1]
          if predicate.is_a?(Array) && predicate[0].is_a?(Symbol)
            name = predicate[0].to_s
            predicate[1..].each do |arg|
              # arg format: [:name, value] - e.g., [:num_args, 1]
              if arg.is_a?(Array) && arg.length == 2
                key = arg[0].to_s
                val = arg[1]
                opts[key] = val
              end
            end
          end
        when :attr?
          # Attribute predicate - extract value constraints
          ast[1..].each { |child| walk_predicates(child, opts) }
        when :val_included_in?
          opts["one_of"] = ast[1..]
        when :filled?
          opts["required"] = true
        end
      end

      def self.build_formula(field, opts)
        sort = type_to_sort(opts["type"])
        v = IR.var(name: field)
        formulas = []

        # Required / filled
        formulas << IR.neq(v, sort == IR::Sort.String ? IR.str("") : IR.num(0)) if opts["required"]

        # Numeric comparisons
        formulas << IR.gte(v, IR.num(opts["gteq?"]))   if opts["gteq?"].is_a?(Integer)
        formulas << IR.lte(v, IR.num(opts["lteq?"]))   if opts["lteq?"].is_a?(Integer)
        formulas << IR.gt(v, IR.num(opts["gt?"]))       if opts["gt?"].is_a?(Integer)
        formulas << IR.lt(v, IR.num(opts["lt?"]))       if opts["lt?"].is_a?(Integer)
        formulas << IR.eq(v, IR.num(opts["eql?"]))      if opts["eql?"].is_a?(Integer)

        # String constraints
        if sort == IR::Sort.String
          str_len = IR.ctor("String.prototype.length", v)
          formulas << IR.gte(str_len, IR.num(opts["min_size?"])) if opts["min_size?"].is_a?(Integer)
          formulas << IR.lte(str_len, IR.num(opts["max_size?"])) if opts["max_size?"].is_a?(Integer)
          formulas << IR.eq(str_len, IR.num(opts["size?"]))      if opts["size?"].is_a?(Integer)
        end

        # Format
        formulas << IR.and() if opts["format?"] # placeholder

        # One-of
        if opts["one_of"].is_a?(Array)
          eqs = opts["one_of"].map { |val| IR.eq(v, IR.str(val.to_s)) }
          formulas << IR.or_(*eqs) unless eqs.empty?
        end

        return nil if formulas.empty?
        formulas.length == 1 ? formulas[0] : IR.and(*formulas)
      end

      def self.type_to_sort(type_name)
        case type_name&.downcase
        when "string" then IR::Sort.String
        when "integer", "int", "float", "decimal" then IR::Sort.Int
        when "bool", "boolean" then IR::Sort.Bool
        else IR::Sort.Ref
        end
      end
    end
  end
end
