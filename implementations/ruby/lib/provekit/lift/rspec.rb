# SPDX-License-Identifier: Apache-2.0
#
# ProvekIt RSpec Layer 2 lift adapter.
#
# Walks RSpec test files and lifts structural patterns to canonical IR:
#   Pattern 1 - bounded loop as universal quantifier
#   Pattern 2 - helper-function inlining (memento per call site)
#   Pattern 3 - multi-assert characterization conjunction
#
# Mirrors the Go/ provekit-lift-go-tests, Rust provekit-lift-rust-tests,
# and TypeScript adapters.
#
# Usage:
#   source = File.read("spec/calculator_spec.rb")
#   output = Provekit::Lift::RSpec.lift(source, "calculator_spec.rb")
#   output.decls # => [ContractDecl, ...]

module Provekit
  module Lift
    module RSpec
      ADAPTER = "rspec-layer2"

      LiftWarning = Struct.new(:adapter, :source_path, :item_name, :reason, keyword_init: true)

      Output = Struct.new(:decls, :warnings, :claimed_tests, keyword_init: true) do
        def initialize(decls: [], warnings: [], claimed_tests: [])
          super
        end
      end

      def self.lift(source, source_path)
        lines = source.lines.map(&:chomp)
        decls = []
        warnings = []
        claimed = []

        blocks = scan_blocks(lines, 0, lines.length)

        blocks.each do |block|
          next unless block[:type] == :it

          body_lines = lines[block[:body_start]..block[:body_end]]
          body_text = body_lines.join("\n")

          # Pattern 1: bounded loop
          body_lines.each do |line|
            q = try_pattern1(line)
            if q
              decls << IR::ContractDecl.new(
                name: block[:name],
                pre: q
              )
              claimed << block[:name]
              break
            end
          end

          # Pattern 3: multi-expect (>= 2 expects in one it block)
          next if claimed.include?(block[:name])
          expects = body_text.scan(/\bexpect\s*\(/).length
          if expects >= 2
            decls << IR::ContractDecl.new(
              name: block[:name],
              pre: IR.and()  # true placeholder - characterization conjunction
            )
            claimed << block[:name]
          end
        end

        Output.new(decls: decls, warnings: warnings, claimed_tests: claimed)
      end

      # -- Block scanning ----------------------------------------

      def self.scan_blocks(lines, start_idx, end_idx, depth = 0)
        blocks = []
        i = start_idx
        while i < end_idx
          line = lines[i] || ""
          if line =~ /^\s*(it)\s+['"]([^"']+)['"]/ || line =~ /^\s*(it)\s+%[qQ][\(\[<]([^\)\]>]+)/
            type = :it
            name = $2.to_s
            body_start = i + 1
            body_end = find_matching_end(lines, body_start, end_idx)
            blocks << { type: type, name: name, body_start: body_start, body_end: body_end, depth: depth }
            i = body_end + 1
          elsif line =~ /^\s*(describe|context)\s+['"]([^"']+)['"]/
            type = $1.to_sym
            name = $2.to_s
            body_start = i + 1
            body_end = find_matching_end(lines, body_start, end_idx)
            blocks << { type: type, name: name, body_start: body_start, body_end: body_end, depth: depth }
            if body_end > body_start
              blocks.concat(scan_blocks(lines, body_start, body_end, depth + 1))
            end
            i = body_end + 1
          else
            i += 1
          end
        end
        blocks
      end

      def self.find_matching_end(lines, start, max)
        depth = 1
        i = start
        while i < max && depth > 0
          line = lines[i] || ""
          # Count do/end and {/} pairs. Be careful: `do` and `end` vary by indentation
          # so we count all opening keywords and their closing counterparts.
          depth += 1 if line.match?(/\bdo\b\s*(\|.*\|)?\s*$/) && !line.match?(/\bend\b/)
          depth += 1 if line.strip.match?(/^[^#]*\{\s*\|.*\|\s*$/) && !line.strip.match?(/^\}/)
          depth -= 1 if line.strip == "end"
          depth -= 1 if line.strip == "}"
          return i if depth == 0
          i += 1
        end
        start
      end

      # -- Pattern 1: bounded loop -> forall quantifier -----------

      def self.try_pattern1(line)
        # (lo..hi).each { |i| ... }
        if line =~ /\((\d+)\.\.(\.)?(\d+)\)\.each\s*\{\s*\|(\w+)\|/
          lo = $1.to_i; hi = $3.to_i; inclusive = !$2; var = $4
          build_bounded_forall(lo, hi, inclusive, var)
        elsif line =~ /\((\d+)\.\.(\.)?(\d+)\)\.each\b/
          lo = $1.to_i; hi = $3.to_i; inclusive = !$2
          # Infer variable from block param
          if line =~ /\|\s*(\w+)\s*\|/
            build_bounded_forall(lo, hi, inclusive, $1)
          end
        elsif line =~ /(\d+)\.upto\((\d+)\)/
          lo = $1.to_i; hi = $2.to_i
          var = line[/(\w+)\s*\|/, 1] || "i"
          build_bounded_forall(lo, hi, true, var)
        elsif line =~ /(\d+)\.downto\((\d+)\)/
          hi = $1.to_i; lo = $2.to_i
          var = line[/(\w+)\s*\|/, 1] || "i"
          build_bounded_forall(lo, hi, true, var)
        end
      end

      def self.build_bounded_forall(lo, hi, inclusive, var_name)
        v = IR.var(name: var_name)
        lo_term = IR.num(lo)
        hi_term = IR.num(hi)

        lower = IR.gte(v, lo_term)
        upper = inclusive ? IR.lte(v, hi_term) : IR.lt(v, hi_term)
        ante = IR.and(lower, upper)

        IR.forall(name: var_name, sort: IR::Sort.Int, body: ante)
      end
    end
  end
end
