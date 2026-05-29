require "json"
require "ripper"
require "provekit/ir"
require "provekit/blake3"
require "provekit/ruby_ast_template"

module Provekit
  class RubyLifter
    IO_RECEIVERS = %w[File IO Dir Net HTTP TCPSocket UDPSocket Socket URI Kernel].freeze
    IO_METHODS = %w[open read write puts print printf gets readline foreach delete rename mkdir rmdir].freeze

    def lift_source(source, path = "source.rb")
      sexp = Ripper.sexp(source)
      comments = comment_roles(source)
      sigs = sorbet_sigs(source)
      methods = Provekit::RubyAstTemplate.extract_methods(source, path)
      declarations = []

      methods.each do |method_info|
        name = method_info[:name]
        params = method_info[:params]
        body = method_info[:body]
        sig = nearest_sig(sigs, method_info[:line])
        roles = nearest_roles(comments, method_info[:line])
        effects = effects_for(body)
        bindings = bindings_for(body)
        ordering = bindings.each_with_index.map do |binding, index|
          { "index" => index, "kind" => "binding", "name" => binding["name"] }
        end
        contract = {
          "kind" => "contract",
          "name" => name,
          "outBinding" => "out",
          "language" => "ruby",
          "surface" => "ruby-source",
          "params" => params,
          "pre" => formula_from_roles(roles["pre"]),
          "post" => formula_from_roles(roles["post"]),
          "inv" => formula_from_roles(roles["invariant"]),
          "evidence" => compact_hash({
            "path" => path,
            "sorbet_sig" => sig,
            "comments" => roles,
            "effects" => effects,
            "values" => bindings,
            "ordering" => ordering
          }),
          "body_source" => method_info[:body_source]
        }
        contract.delete("pre") unless contract["pre"]
        contract.delete("post") unless contract["post"]
        contract.delete("inv") unless contract["inv"]
        contract["contract_cid"] = cid_for(contract_cid_payload(contract))
        declarations << contract
      end

      {
        "kind" => "ir-document",
        "version" => "provekit-ir/1.1.0",
        "source_language" => "ruby",
        "declarations" => declarations,
        "diagnostics" => sexp ? [] : [{ "severity" => "warning", "message" => "Ripper returned no AST" }]
      }
    end

    def cid_for(value)
      Provekit::Blake3.hex(Provekit::IR::Jcs.encode(value))
    end

    private

    def comment_roles(source)
      roles = []
      source.each_line.with_index(1) do |line, number|
        if line =~ /^\s*#\s*@(pre|post|invariant)\s+(.+?)\s*$/
          roles << { "role" => Regexp.last_match(1), "text" => Regexp.last_match(2), "line" => number }
        end
      end
      roles
    end

    def sorbet_sigs(source)
      sigs = []
      lines = source.lines
      index = 0
      while index < lines.length
        if lines[index] =~ /^\s*sig\s*(\{.*\})\s*$/
          text = lines[index].strip
          params = {}
          if text =~ /params\((.*?)\)/
            Regexp.last_match(1).split(/\s*,\s*/).each do |pair|
              key, value = pair.split(/\s*:\s*/, 2)
              params[key] = value if key && value
            end
          end
          returns = text[/\.returns\(([^)]+)\)/, 1]
          sigs << { "line" => index + 1, "source" => text, "params" => params, "returns" => returns }
        end
        index += 1
      end
      sigs
    end

    def method_regions(source)
      regions = []
      lines = source.lines
      index = 0
      while index < lines.length
        line = lines[index]
        if line =~ /^\s*def\s+([a-zA-Z_][\w!?=]*)\s*(?:\(([^)]*)\))?/
          name = Regexp.last_match(1)
          params = parse_params(Regexp.last_match(2))
          start_line = index + 1
          depth = 1
          body_lines = []
          index += 1
          while index < lines.length
            current = lines[index]
            depth += 1 if current =~ /^\s*(def|class|module|begin|if|unless|case|while|until|for)\b/ || current =~ /\bdo\s*(\|.*\|)?\s*$/
            depth -= 1 if current =~ /^\s*end\s*$/
            break if depth == 0
            body_lines << current
            index += 1
          end
          regions << { name: name, params: params, line: start_line, body: body_lines.join }
        end
        index += 1
      end
      regions
    end

    def parse_params(text)
      return [] if text.nil? || text.strip.empty?
      text.split(/\s*,\s*/).map do |param|
        param.sub(/:.*/, "").sub(/=.*/, "").sub(/^\*/, "").sub(/^&/, "").strip
      end.reject(&:empty?)
    end

    def nearest_sig(sigs, method_line)
      sigs.select { |sig| sig["line"] < method_line }.max_by { |sig| sig["line"] }
    end

    def nearest_roles(roles, method_line)
      selected = { "pre" => [], "post" => [], "invariant" => [] }
      roles.select { |role| role["line"] < method_line && method_line - role["line"] <= 12 }.each do |role|
        selected[role["role"]] << role["text"]
      end
      selected
    end

    def effects_for(body)
      effects = []
      body.each_line.with_index(1) do |line, index|
        effects << { "kind" => "Panics", "line" => index, "surface" => line.strip } if line =~ /\braise\b/
        if io_line?(line)
          effects << { "kind" => "Io", "line" => index, "surface" => line.strip }
        end
      end
      effects
    end

    def io_line?(line)
      IO_RECEIVERS.any? { |receiver| line.include?(receiver + ".") } ||
        IO_METHODS.any? { |meth| line =~ /^\s*#{Regexp.escape(meth)}\b/ }
    end

    def bindings_for(body)
      bindings = []
      body.each_line.with_index(1) do |line, index|
        next unless line =~ /^\s*([a-z_]\w*)\s*=\s*(.+?)\s*$/
        bindings << { "name" => Regexp.last_match(1), "line" => index, "term" => Regexp.last_match(2).strip }
      end
      bindings
    end

    def formula_from_roles(items)
      return nil if items.nil? || items.empty?
      clauses = items.map { |text| { "kind" => "atomic", "name" => "ruby_predicate", "args" => [text] } }
      clauses.length == 1 ? clauses.first : { "kind" => "and", "operands" => clauses }
    end

    def compact_hash(hash)
      hash.each_with_object({}) do |(key, value), out|
        next if value.nil? || value == [] || value == {}
        out[key] = value
      end
    end

    def contract_cid_payload(contract)
      copy = Marshal.load(Marshal.dump(contract))
      copy.delete("contract_cid")
      copy.dig("evidence")&.delete("path")
      if (body_source = copy["body_source"])
        body_source.delete("file")
        body_source.delete("span")
        body_source.delete("source_cid")
      end
      copy
    end
  end
end
