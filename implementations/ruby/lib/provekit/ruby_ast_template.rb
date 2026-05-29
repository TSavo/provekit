require "ripper"

require "provekit/blake3"
require "provekit/ir"

module Provekit
  module RubyAstTemplate
    module_function

    def extract_methods(source, file = "source.rb")
      sexp = Ripper.sexp(source)
      return [] unless sexp

      lines = source.lines
      collect_defs(sexp).map do |node|
        method_name = def_name(node)
        params = def_params(node)
        start_line = def_line(node)
        next unless method_name && start_line

        start_index = start_line - 1
        end_index = find_matching_end(lines, start_index)
        body_text = dedent(lines[(start_index + 1)...end_index].join)
        span_text = lines[start_index..end_index].join
        body_source = body_source_for_body(
          body_text,
          params,
          file: file,
          span: {
            "start_line" => start_line,
            "start_col" => leading_spaces(lines[start_index]),
            "end_line" => end_index + 1,
            "end_col" => lines[end_index].to_s.length,
          },
          source_text: span_text,
        )

        {
          name: method_name,
          params: params,
          line: start_line,
          body: body_text,
          body_source: body_source,
        }
      end.compact.sort_by { |info| info[:line] }
    end

    def body_source_for_body(body_text, params = [], file: "source.rb", span: nil, source_text: nil)
      text = dedent(body_text.to_s)
      template = ast_template_for_body(text, params)
      {
        "file" => file,
        "span" => span || default_span(text),
        "source_cid" => cid_for(source_text || text),
        "body_text" => text,
        "ast_template" => template,
        "template_cid" => cid_for(template),
        "param_names" => params.map(&:to_s),
      }
    end

    def ast_template_for_body(body_text, params = [])
      text = body_text.to_s
      wrapper = +"def __provekit_template__(#{params.map(&:to_s).join(", ")})\n"
      wrapper << indent_body(text)
      wrapper << "\n" unless wrapper.end_with?("\n")
      wrapper << "end\n"

      sexp = Ripper.sexp(wrapper)
      statements = sexp ? body_statements_from_wrapper(sexp) : nil
      if statements.nil?
        return {
          "kind" => "ruby:block",
          "stmts" => [{ "kind" => "unparsed", "source" => text.strip }],
        }
      end

      {
        "kind" => "ruby:block",
        "stmts" => Array(statements).map { |stmt| canonicalize(stmt, params) },
      }
    end

    def cid_for(value)
      bytes = value.is_a?(String) ? value : Provekit::IR::Jcs.encode(value)
      Provekit::Blake3.hex(bytes)
    end

    def dedent(text)
      lines = text.to_s.lines
      lines.shift while lines.first&.strip == ""
      lines.pop while lines.last&.strip == ""
      width = lines.reject { |line| line.strip == "" }.map { |line| leading_spaces(line) }.min || 0
      lines.map { |line| line.sub(/\A[ \t]{0,#{width}}/, "") }.join.chomp
    end

    def indent_body(text)
      return "" if text.to_s.empty?

      text.lines.map { |line| line.strip.empty? ? line : "  #{line}" }.join
    end

    def default_span(text)
      line_count = [text.lines.length, 1].max
      {
        "start_line" => 1,
        "start_col" => 0,
        "end_line" => line_count,
        "end_col" => text.lines.last.to_s.length,
      }
    end

    def leading_spaces(line)
      line.to_s[/\A[ \t]*/].to_s.length
    end

    def collect_defs(node, out = [])
      return out unless node.is_a?(Array)

      kind = node_kind(node)
      out << node if kind == :def || kind == :defs
      node.each { |child| collect_defs(child, out) if child.is_a?(Array) }
      out
    end

    def def_name(node)
      case node_kind(node)
      when :def
        token_text(node[1])
      when :defs
        receiver = receiver_name(node[1])
        name = token_text(node[3])
        receiver && receiver != "" ? "#{receiver}.#{name}" : name
      end
    end

    def receiver_name(node)
      case node_kind(node)
      when :var_ref, :vcall
        token_text(node[1])
      when :@kw, :@ident, :@const
        token_text(node)
      else
        nil
      end
    end

    def def_params(node)
      params_node = node_kind(node) == :defs ? node[4] : node[2]
      params_node = params_node[1] if node_kind(params_node) == :paren
      return [] unless node_kind(params_node) == :params

      required = Array(params_node[1])
      required.map { |item| token_text(item) }.compact
    end

    def def_line(node)
      name_node = node_kind(node) == :defs ? node[3] : node[1]
      token_line(name_node)
    end

    def find_matching_end(lines, start_index)
      depth = 0
      i = start_index
      while i < lines.length
        line = lines[i].to_s
        depth += line.scan(/^\s*(def|class|module|begin|if|unless|case|while|until|for)\b/).length
        depth += 1 if line =~ /\bdo\s*(\|.*\|)?\s*$/
        if line.strip == "end"
          depth -= 1
          return i if depth <= 0
        end
        i += 1
      end
      [lines.length - 1, start_index].max
    end

    def body_statements_from_wrapper(sexp)
      def_node = Array(sexp[1]).find { |node| node_kind(node) == :def }
      return nil unless def_node

      body = def_node[3]
      return [] if body.nil?
      return nil unless node_kind(body) == :bodystmt

      body[1] || []
    end

    def canonicalize(node, params)
      case node
      when Array
        return canonicalize_token(node, params) if token_node?(node)

        kind = node_kind(node)
        unless kind.is_a?(Symbol)
          return {
            "kind" => "list",
            "children" => node.map { |child| canonicalize(child, params) },
          }
        end

        {
          "kind" => kind.to_s,
          "children" => Array(node[1..]).map { |child| canonicalize(child, params) },
        }
      when Symbol
        { "kind" => "symbol", "name" => node.to_s }
      when String, Integer, Float, TrueClass, FalseClass, NilClass
        node
      else
        node.to_s
      end
    end

    def canonicalize_token(node, params)
      text = node[1].to_s
      index = params.map(&:to_s).index(text)
      return { "kind" => "param_ref", "index" => index + 1 } if index

      type = node[0].to_s.delete_prefix("@")
      case node[0]
      when :@int, :@float, :@rational, :@imaginary, :@tstring_content
        { "kind" => "literal", "type" => type, "value" => text }
      else
        { "kind" => "token", "type" => type, "text" => text }
      end
    end

    def token_node?(node)
      node.length >= 3 && node[0].is_a?(Symbol) && node[0].to_s.start_with?("@")
    end

    def node_kind(node)
      node.is_a?(Array) ? node[0] : nil
    end

    def token_text(node)
      token_node?(node) ? node[1].to_s : nil
    end

    def token_line(node)
      token_node?(node) && node[2].is_a?(Array) ? node[2][0].to_i : nil
    end
  end
end
