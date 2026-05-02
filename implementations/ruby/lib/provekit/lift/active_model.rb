# SPDX-License-Identifier: Apache-2.0
#
# ProvekIt ActiveModel lift adapter.
#
# Walks Ruby classes with ActiveModel::Validations and lifts
# validation declarations to canonical IR.
#
# Mirrors: Java Bean Validation, Python pydantic, C# DataAnnotations.
#
# Example:
#   class User
#     include ActiveModel::Validations
#     validates :name, presence: true, length: { minimum: 1, maximum: 100 }
#     validates :age, numericality: { greater_than_or_equal_to: 0, less_than_or_equal_to: 150 }
#   end
#
#   decls = Provekit::Lift::ActiveModel.lift(User)
#   # => [ContractDecl(name: "User.name", pre: ...), ...]

module Provekit
  module Lift
    module ActiveModel
      def self.lift(klass)
        decls = []
        class_name = klass.name&.split("::")&.last || klass.to_s

        # Use _validators if available (ActiveModel::Validations)
        if klass.respond_to?(:_validators)
          klass._validators.each do |field, validators|
            formulas = []
            var = IR.var(name: field.to_s)
            sort = resolve_sort(klass, field)

            validators.each do |v|
              f = lift_validator(var, sort, v)
              formulas << f if f
            end

            next if formulas.empty?

            pre = formulas.length == 1 ? formulas[0] : IR.and(*formulas)
            decls << IR::ContractDecl.new(name: "#{class_name}.#{field}", pre: pre)
          end
        end

        # Also check validates() class-level macros (Rails model style)
        if klass.respond_to?(:_validate_callbacks) || klass.respond_to?(:validators)
          # covered above via _validators
        end

        decls
      end

      def self.lift_validator(var, sort, validator)
        kind = validator.kind.to_s

        case kind
        when "presence"
          IR.neq(var, sort == IR::Sort.String ? IR.str("") : IR.num(0))

        when "numericality"
          opts = validator.options
          formulas = []
          formulas << IR.gte(var, IR.num(opts[:greater_than_or_equal_to])) if opts[:greater_than_or_equal_to]
          formulas << IR.lte(var, IR.num(opts[:less_than_or_equal_to]))     if opts[:less_than_or_equal_to]
          formulas << IR.gt(var, IR.num(opts[:greater_than]))               if opts[:greater_than]
          formulas << IR.lt(var, IR.num(opts[:less_than]))                  if opts[:less_than]
          formulas << IR.eq(var, IR.num(opts[:equal_to]))                   if opts[:equal_to]
          formulas << IR.neq(var, IR.num(opts[:other_than]))                if opts[:other_than]
          formulas.empty? ? nil : IR.and(*formulas)

        when "length"
          opts = validator.options
          str_len = IR.ctor("String.prototype.length", var)
          formulas = []
          formulas << IR.gte(str_len, IR.num(opts[:minimum]))               if opts[:minimum]
          formulas << IR.lte(str_len, IR.num(opts[:maximum]))               if opts[:maximum]
          formulas << IR.eq(str_len, IR.num(opts[:is]))                     if opts[:is]
          formulas << IR.gte(str_len, IR.num(opts[:in]&.min))               if opts[:in].is_a?(Range)
          formulas << IR.lte(str_len, IR.num(opts[:in]&.max))               if opts[:in].is_a?(Range)
          formulas.empty? ? nil : IR.and(*formulas)

        when "format"
          pattern = validator.options[:with].to_s
          pattern.empty? ? nil : IR.and() # placeholder

        when "inclusion"
          values = [validator.options[:in]].flatten.compact
          return nil if values.empty?
          eqs = values.map { |v| IR.eq(var, IR.respond_to?(:str) ? IR.str(v.to_s) : IR.num(v.to_i)) }
          IR.or_(*eqs)

        when "exclusion"
          values = [validator.options[:in]].flatten.compact
          return nil if values.empty?
          eqs = values.map { |v| IR.neq(var, IR.respond_to?(:str) ? IR.str(v.to_s) : IR.num(v.to_i)) }
          IR.and(*eqs)

        else
          nil # unrecognized validator
        end
      end

      def self.resolve_sort(klass, field)
        # Try to infer sort from column type (ActiveRecord) or attribute type
        if klass.respond_to?(:columns_hash) && klass.columns_hash[field.to_s]
          col = klass.columns_hash[field.to_s]
          case col.type
          when :string, :text then IR::Sort.String
          when :integer, :float, :decimal then IR::Sort.Int
          when :boolean then IR::Sort.Bool
          else IR::Sort.Ref
          end
        elsif klass.respond_to?(:attribute_types) && klass.attribute_types[field.to_s]
          type = klass.attribute_types[field.to_s].type.to_s
          case type
          when "string", "text" then IR::Sort.String
          when "integer", "float", "decimal" then IR::Sort.Int
          when "boolean" then IR::Sort.Bool
          else IR::Sort.Ref
          end
        else
          IR::Sort.Ref
        end
      end
    end
  end
end
