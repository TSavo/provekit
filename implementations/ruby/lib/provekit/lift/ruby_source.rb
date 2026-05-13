# SPDX-License-Identifier: Apache-2.0

require "base64"
require "find"
require "fileutils"
require "json"
require "pathname"
require "ripper"
require "set"

require_relative "../blake3"
require_relative "../ir"

module Provekit
  module Lift
    module RubySource
      SURFACE = "ruby-source".freeze
      VERSION = "0.1.0-draft".freeze
      IR_VERSION = "v1.1.0".freeze

      LiftResult = Struct.new(:ir, :diagnostics, :opacity_report, :refusals, keyword_init: true) do
        def initialize(ir: [], diagnostics: [], opacity_report: [], refusals: [])
          super
        end
      end

      FunctionInfo = Struct.new(:node, :fn_name, :method_name, :formals, :body, :line, keyword_init: true)

      class UnsupportedSyntax < StandardError
        attr_reader :node, :reason, :kind

        def initialize(node, reason, kind: "unhandled-syntax")
          @node = node
          @reason = reason
          @kind = kind
          super(reason)
        end
      end

      class EffectSet
        def initialize
          @effects = {}
        end

        def add_reads(target)
          add(["reads", target], { "kind" => "reads", "target" => target })
        end

        def add_writes(target)
          add(["writes", target], { "kind" => "writes", "target" => target })
        end

        def add_io
          add(["io", ""], { "kind" => "io" })
        end

        def add_panics
          add(["panics", ""], { "kind" => "panics" })
        end

        def add_unresolved_call(name)
          add(["unresolved_call", name], { "kind" => "unresolved_call", "name" => name })
        end

        def add_opaque_loop(loop_term)
          loop_cid = RubySource.cid_of_json(loop_term)
          add(["opaque_loop", loop_cid], { "kind" => "opaque_loop", "loopCid" => loop_cid })
        end

        def sorted
          @effects.values.sort_by { |effect| RubySource.effect_sort_key(effect) }
        end

        private

        def add(key, effect)
          @effects[key] = effect
        end
      end

      class DefinitionCollector
        attr_reader :definitions, :refusals

        def initialize
          @definitions = []
          @refusals = []
          @seen_namespaces = Set.new
          @seen_functions = Set.new
        end

        def collect(sexp)
          unless RubySource.node_kind(sexp) == :program
            add_refusal(nil, "top-level parse did not produce a Ruby program", node: sexp)
            return
          end
          visit_statements(sexp[1] || [], [])
        end

        private

        def visit_statements(statements, scope)
          Array(statements).each do |statement|
            next if RubySource.empty_statement?(statement)

            case RubySource.node_kind(statement)
            when :module
              visit_namespace(statement, scope, "module")
            when :class
              visit_namespace(statement, scope, "class")
            when :def
              record_instance_method(statement, scope)
            when :defs
              record_singleton_method(statement, scope)
            else
              add_refusal(nil, "unsupported top-level/class/module statement: #{RubySource.node_kind(statement)}", node: statement)
            end
          end
        end

        def visit_namespace(node, scope, kind)
          name = RubySource.const_name(node[1])
          if kind == "class" && !node[2].nil?
            add_refusal(nil, "class inheritance clauses are refused by the draft Ruby source lifter", node: node[2])
            return
          end
          full_scope = scope + name.split("::")
          key = [kind, full_scope.join("::")]
          if @seen_namespaces.include?(key)
            add_refusal(nil, "module/class reopening is refused: #{full_scope.join("::")}", node: node)
            return
          end
          @seen_namespaces.add(key)
          statements = RubySource.statements_from_bodystmt(node[kind == "module" ? 2 : 3])
          visit_statements(statements, full_scope)
        rescue UnsupportedSyntax => e
          add_refusal(nil, e.reason, node: e.node, kind: e.kind)
        end

        def record_instance_method(node, scope)
          method_name = RubySource.token_text(node[1])
          namespace = scope.empty? ? "<top>" : scope.join("::")
          fn_name = "#{namespace}##{method_name}"
          record_method(node, fn_name, method_name, node[2], node[3])
        end

        def record_singleton_method(node, scope)
          receiver = node[1]
          method_name = RubySource.token_text(node[3])
          receiver_name =
            if RubySource.self_receiver?(receiver)
              scope.empty? ? "<top>" : scope.join("::")
            elsif RubySource.const_like?(receiver)
              RubySource.const_name(receiver)
            else
              raise UnsupportedSyntax.new(receiver, "singleton method receiver is not a static self/constant")
            end
          fn_name = "#{receiver_name}.#{method_name}"
          record_method(node, fn_name, method_name, node[4], node[5])
        rescue UnsupportedSyntax => e
          add_refusal(nil, e.reason, node: e.node, kind: e.kind)
        end

        def record_method(node, fn_name, method_name, params_node, body_node)
          if @seen_functions.include?(fn_name)
            add_refusal(fn_name, "duplicate Ruby method definition is refused: #{fn_name}", node: node, kind: "duplicate-method")
            return
          end
          formals = RubySource.parse_params(params_node)
          body = RubySource.statements_from_bodystmt(body_node)
          @seen_functions.add(fn_name)
          @definitions << FunctionInfo.new(
            node: node,
            fn_name: fn_name,
            method_name: method_name,
            formals: formals,
            body: body,
            line: RubySource.line_of(node),
          )
        rescue UnsupportedSyntax => e
          add_refusal(fn_name, e.reason, node: e.node, kind: e.kind)
        end

        def add_refusal(function, reason, node:, kind: "unhandled-syntax")
          @refusals << {
            "kind" => kind,
            "function" => function,
            "line" => RubySource.line_of(node),
            "reason" => reason,
          }
        end
      end

      class Emitter
        def initialize(effects:)
          @effects = effects
        end

        def statements(statements)
          terms = Array(statements).reject { |statement| RubySource.empty_statement?(statement) }.map do |statement|
            statement(statement)
          end
          RubySource.fold_seq(terms)
        end

        def statement(node)
          case RubySource.node_kind(node)
          when :assign
            target = target(node[1])
            record_write(node[1])
            RubySource.ctor("ruby:assign", target, expr(node[2]))
          when :return
            values = args(node[1])
            raise UnsupportedSyntax.new(node, "multiple return values are refused") if values.length > 1

            RubySource.ctor("ruby:return", values.empty? ? RubySource.none_const : values.first)
          when :if
            RubySource.ctor(
              "ruby:if",
              expr(node[1]),
              statements(node[2] || []),
              else_statements(node[3]),
            )
          when :if_mod
            RubySource.ctor("ruby:if", expr(node[1]), statement(node[2]), RubySource.empty_stmt)
          when :while
            term = RubySource.ctor("ruby:while", expr(node[1]), statements(node[2] || []))
            @effects.add_opaque_loop(term)
            term
          when :until
            term = RubySource.ctor("ruby:until", expr(node[1]), statements(node[2] || []))
            @effects.add_opaque_loop(term)
            term
          when :for
            term = RubySource.ctor("ruby:for", target(node[1]), expr(node[2]), statements(node[3] || []))
            @effects.add_opaque_loop(term)
            term
          when :command, :method_add_arg
            if raise_call?(node)
              @effects.add_panics
              values = call_args(node)
              raise UnsupportedSyntax.new(node, "raise with more than one argument is refused") if values.length > 1

              RubySource.ctor("ruby:raise", values.empty? ? RubySource.none_const : values.first)
            else
              RubySource.ctor("ruby:expr", expr(node))
            end
          when :begin
            raise UnsupportedSyntax.new(node, "begin/rescue/ensure blocks are refused")
          when :break, :next, :redo, :retry
            raise UnsupportedSyntax.new(node, "loop control statement #{RubySource.node_kind(node)} is not modeled by the draft lifter")
          else
            RubySource.ctor("ruby:expr", expr(node))
          end
        end

        def target(node)
          case RubySource.node_kind(node)
          when :var_field
            variable_token(node[1])
          when :aref_field
            actuals = args(node[2])
            raise UnsupportedSyntax.new(node, "index assignment with more than one index is refused") unless actuals.length == 1

            RubySource.ctor("ruby:index", expr(node[1]), actuals.first)
          else
            raise UnsupportedSyntax.new(node, "unsupported assignment target: #{RubySource.node_kind(node)}")
          end
        end

        def expr(node)
          case RubySource.node_kind(node)
          when :@int
            RubySource.int_const(RubySource.token_text(node).to_i)
          when :@ident
            RubySource.var(RubySource.token_text(node))
          when :@kw
            keyword(RubySource.token_text(node), node)
          when :var_ref
            variable_token(node[1])
          when :const_ref, :const_path_ref, :top_const_ref
            name = RubySource.const_name(node)
            @effects.add_reads(name)
            RubySource.ctor("ruby:const", RubySource.str_const(name))
          when :vcall
            call_term(RubySource.token_text(node[1]), [])
          when :fcall
            call_term(RubySource.token_text(node[1]), [])
          when :binary
            op = RubySource.binary_op(node[2], node)
            RubySource.ctor(op, expr(node[1]), expr(node[3]))
          when :unary
            op = RubySource.unary_op(node[1], node)
            RubySource.ctor(op, expr(node[2]))
          when :ifop
            RubySource.ctor("ruby:ternary", expr(node[1]), expr(node[2]), expr(node[3]))
          when :paren
            inner = Array(node[1]).reject { |statement| RubySource.empty_statement?(statement) }
            return RubySource.none_const if inner.empty?
            raise UnsupportedSyntax.new(node, "parenthesized expression with multiple statements is refused") unless inner.length == 1

            expr(inner.first)
          when :string_literal
            RubySource.str_const(static_string_content(node[1]))
          when :aref
            actuals = args(node[2])
            raise UnsupportedSyntax.new(node, "index expression with more than one index is refused") unless actuals.length == 1

            RubySource.ctor("ruby:index", expr(node[1]), actuals.first)
          when :call
            receiver, period, method = node[1], node[2], node[3]
            raise UnsupportedSyntax.new(node, "safe navigation (&.) is refused") if RubySource.token_text(period) == "&."

            send_term(receiver, RubySource.token_text(method), [])
          when :method_add_arg
            call_with_args(node[1], args_from_arg_node(node[2]))
          when :command
            call_term(RubySource.token_text(node[1]), args(node[2]))
          when :method_add_block, :brace_block, :do_block, :lambda
            raise UnsupportedSyntax.new(node, "blocks/procs/lambdas are refused")
          when :yield
            raise UnsupportedSyntax.new(node, "yield is refused")
          when :array, :hash
            raise UnsupportedSyntax.new(node, "#{RubySource.node_kind(node)} literals are not modeled by the draft lifter")
          when :rescue_mod
            raise UnsupportedSyntax.new(node, "rescue modifier is refused")
          else
            raise UnsupportedSyntax.new(node, "unhandled expression kind: #{RubySource.node_kind(node)}")
          end
        end

        private

        def else_statements(node)
          return RubySource.empty_stmt if node.nil?

          case RubySource.node_kind(node)
          when :else
            statements(node[1] || [])
          else
            raise UnsupportedSyntax.new(node, "elsif branches are refused by the draft lifter")
          end
        end

        def keyword(value, node)
          case value
          when "true"
            RubySource.bool_const(true)
          when "false"
            RubySource.bool_const(false)
          when "nil"
            RubySource.none_const
          when "self"
            RubySource.var("self")
          else
            raise UnsupportedSyntax.new(node, "unsupported keyword expression: #{value}")
          end
        end

        def variable_token(token)
          kind = RubySource.node_kind(token)
          text = RubySource.token_text(token)
          case kind
          when :@ident
            RubySource.var(text)
          when :@ivar
            RubySource.ctor("ruby:ivar", RubySource.str_const(text))
          when :@gvar
            @effects.add_reads(text)
            RubySource.ctor("ruby:gvar", RubySource.str_const(text))
          when :@cvar
            @effects.add_reads(text)
            RubySource.ctor("ruby:cvar", RubySource.str_const(text))
          when :@const
            @effects.add_reads(text)
            RubySource.ctor("ruby:const", RubySource.str_const(text))
          when :@kw
            keyword(text, token)
          else
            raise UnsupportedSyntax.new(token, "unsupported variable token: #{kind}")
          end
        end

        def static_string_content(node)
          return "" if node.nil?
          raise UnsupportedSyntax.new(node, "string interpolation is refused") unless RubySource.node_kind(node) == :string_content

          parts = node[1..] || []
          parts.map do |part|
            if RubySource.node_kind(part) == :@tstring_content
              RubySource.token_text(part)
            else
              raise UnsupportedSyntax.new(part, "string interpolation is refused")
            end
          end.join
        end

        def args(node)
          args_from_arg_node(node)
        end

        def args_from_arg_node(node)
          return [] if node.nil? || node == false

          case RubySource.node_kind(node)
          when :arg_paren
            args_from_arg_node(node[1])
          when :args_add_block
            raise UnsupportedSyntax.new(node, "block call arguments are refused") unless node[2] == false || node[2].nil?

            Array(node[1]).map { |arg| argument_expr(arg) }
          else
            raise UnsupportedSyntax.new(node, "unsupported argument list: #{RubySource.node_kind(node)}")
          end
        end

        def argument_expr(node)
          case RubySource.node_kind(node)
          when :bare_assoc_hash
            raise UnsupportedSyntax.new(node, "keyword arguments are refused")
          when :args_add_star
            raise UnsupportedSyntax.new(node, "splat arguments are refused")
          else
            expr(node)
          end
        end

        def call_args(node)
          case RubySource.node_kind(node)
          when :command
            args(node[2])
          when :method_add_arg
            args_from_arg_node(node[2])
          else
            []
          end
        end

        def call_with_args(call_node, actuals)
          case RubySource.node_kind(call_node)
          when :fcall
            call_term(RubySource.token_text(call_node[1]), actuals)
          when :vcall
            call_term(RubySource.token_text(call_node[1]), actuals)
          when :call
            receiver, period, method = call_node[1], call_node[2], call_node[3]
            raise UnsupportedSyntax.new(call_node, "safe navigation (&.) is refused") if RubySource.token_text(period) == "&."

            send_term(receiver, RubySource.token_text(method), actuals)
          else
            raise UnsupportedSyntax.new(call_node, "unsupported callee kind: #{RubySource.node_kind(call_node)}")
          end
        end

        def call_term(callee, actuals)
          refuse_dynamic_or_meta_call(callee)
          record_call_effect(callee)
          RubySource.ctor("ruby:call", RubySource.str_const(callee), *actuals)
        end

        def send_term(receiver_node, method, actuals)
          refuse_dynamic_or_meta_call(method)
          receiver = expr(receiver_node)
          display = "#{RubySource.term_to_source(receiver)}.#{method}"
          record_call_effect(display)
          RubySource.ctor("ruby:send", receiver, RubySource.str_const(method), *actuals)
        end

        def record_call_effect(name)
          if RubySource.io_call?(name)
            @effects.add_io
          elsif name == "raise"
            @effects.add_panics
          else
            @effects.add_unresolved_call(name)
          end
        end

        def refuse_dynamic_or_meta_call(name)
          if RubySource::METAPROGRAMMING_CALLS.include?(name)
            raise UnsupportedSyntax.new(nil, "metaprogramming call is refused: #{name}")
          end
        end

        def raise_call?(node)
          case RubySource.node_kind(node)
          when :command
            RubySource.token_text(node[1]) == "raise"
          when :method_add_arg
            call_node = node[1]
            RubySource.node_kind(call_node) == :fcall && RubySource.token_text(call_node[1]) == "raise"
          else
            false
          end
        end

        def record_write(target_node)
          case RubySource.node_kind(target_node)
          when :var_field
            token = target_node[1]
            text = RubySource.token_text(token)
            case RubySource.node_kind(token)
            when :@gvar, :@cvar, :@const
              @effects.add_writes(text)
            end
          when :aref_field
            @effects.add_writes(RubySource.term_to_source(target(target_node)))
          end
        end
      end

      METAPROGRAMMING_CALLS = Set.new(%w[define_method method_missing send __send__ public_send eval instance_eval class_eval module_eval instance_variable_get instance_variable_set const_get const_set]).freeze

      BINOPS = {
        :+ => "ruby:add",
        :- => "ruby:sub",
        :* => "ruby:mul",
        :/ => "ruby:div",
        :% => "ruby:mod",
        :** => "ruby:pow",
        :== => "ruby:eq",
        :!= => "ruby:ne",
        :< => "ruby:lt",
        :<= => "ruby:le",
        :> => "ruby:gt",
        :>= => "ruby:ge",
        :& => "ruby:bitand",
        :| => "ruby:bitor",
        :^ => "ruby:bitxor",
        :<< => "ruby:shl",
        :>> => "ruby:shr",
        :"&&" => "ruby:and",
        :"||" => "ruby:or",
        :and => "ruby:and",
        :or => "ruby:or",
      }.freeze

      UNARYOPS = {
        :-@ => "ruby:neg",
        :+@ => "ruby:pos",
        :! => "ruby:not",
        :not => "ruby:not",
        :~ => "ruby:bitnot",
      }.freeze

      IO_CALLS = Set.new(%w[puts print p gets STDOUT.write STDERR.write STDIN.gets File.read File.write File.open]).freeze

      class << self
        def initialize_result
          {
            "name" => "provekit-lift-ruby-source",
            "version" => VERSION,
            "protocol_version" => "pep/1.7.0",
            "dialect" => SURFACE,
            "capabilities" => {
              "authoring_surfaces" => [SURFACE],
              "ir_version" => IR_VERSION,
              "emits_signed_mementos" => false,
            },
          }
        end

        def run_rpc(stdin: $stdin, stdout: $stdout)
          stdin.binmode if stdin.respond_to?(:binmode)
          stdout.sync = true if stdout.respond_to?(:sync=)

          stdin.each_line do |line|
            line = line.strip
            next if line.empty?

            response =
              begin
                dispatch(JSON.parse(line))
              rescue JSON::ParserError => e
                error(nil, -32700, "PARSE_ERROR: #{e.message}")
              rescue StandardError => e
                error(nil, -32603, "#{e.class}: #{e.message}")
              end
            stdout.write(JSON.generate(response))
            stdout.write("\n")
          end
          0
        end

        def dispatch(request)
          id = request["id"]
          method = request["method"].to_s
          params = request["params"] || {}

          case method
          when "initialize"
            { "jsonrpc" => "2.0", "id" => id, "result" => initialize_result }
          when "lift"
            rpc_lift(id, params)
          when "compile"
            rpc_compile(id, params)
          when "shutdown"
            { "jsonrpc" => "2.0", "id" => id, "result" => nil }
          else
            error(id, -32601, "METHOD_NOT_FOUND: #{method}")
          end
        end

        def lift_source(source, source_path)
          result = LiftResult.new
          sexp = Ripper.sexp(source)
          if sexp.nil?
            result.refusals << {
              "kind" => "syntax-error",
              "function" => nil,
              "line" => nil,
              "reason" => "Ripper could not parse Ruby source",
            }
            result.ir << source_unit_contract(source_path: source_path, source: source, operational_term: empty_stmt)
            return result
          end

          collector = DefinitionCollector.new
          collector.collect(sexp)
          result.refusals.concat(collector.refusals)

          body_terms = []
          contracts = []
          collector.definitions.each do |info|
            contract = lift_function(info, source_path, result)
            next if contract.nil?

            body_terms << contract["post"]["args"][1]
            contracts << contract
          end

          result.ir << source_unit_contract(
            source_path: source_path,
            source: source,
            operational_term: fold_seq(body_terms),
          )
          result.ir.concat(contracts)
          result
        end

        def lift_paths(workspace_root, source_paths)
          result = LiftResult.new
          root = Pathname.new(workspace_root.to_s.empty? ? "." : workspace_root).realpath
          Array(source_paths).each do |requested|
            path = Pathname.new(requested.to_s)
            full = path.absolute? ? path : root + path
            begin
              resolved = full.realpath
            rescue SystemCallError => e
              result.refusals << { "kind" => "io-error", "function" => nil, "line" => nil, "reason" => "cannot resolve path '#{requested}': #{e.message}" }
              next
            end
            unless relative_to?(resolved, root)
              result.refusals << { "kind" => "path-traversal", "function" => nil, "line" => nil, "reason" => "path '#{requested}' escapes workspace root '#{root}'" }
              next
            end
            ruby_files(resolved).each do |file_path|
              source = File.read(file_path, encoding: "UTF-8")
              display_path = file_path.relative_path_from(root).to_s
              file_result = lift_source(source, display_path)
              result.ir.concat(file_result.ir)
              result.diagnostics.concat(file_result.diagnostics)
              result.opacity_report.concat(file_result.opacity_report)
              result.refusals.concat(file_result.refusals)
            rescue SystemCallError => e
              result.refusals << { "kind" => "io-error", "function" => nil, "line" => nil, "reason" => "cannot read '#{file_path}': #{e.message}" }
            end
          end
          result
        end

        def compile_ir_document(ir)
          functions = Array(ir).select do |item|
            item.is_a?(Hash) && item["kind"] == "function-contract" && !source_unit_contract?(item)
          end
          functions.map { |contract| compile_contract(contract) }.join
        end

        def compile_body_term(term, fn_name: "f", formals: [])
          contract = {
            "kind" => "function-contract",
            "fnName" => fn_name.include?("#") || fn_name.include?(".") ? fn_name : "<top>##{fn_name}",
            "formals" => formals,
            "post" => { "args" => [nil, term] },
          }
          compile_contract(contract)
        end

        def cid_of_json(value)
          Provekit::Blake3.hex(Provekit::IR::Jcs.encode(value))
        end

        def effect_sort_key(effect)
          case effect["kind"]
          when "reads"
            "0:reads:#{effect["target"]}"
          when "writes"
            "1:writes:#{effect["target"]}"
          when "io"
            "2:io"
          when "unsafe"
            "3:unsafe"
          when "panics"
            "4:panics"
          when "unresolved_call"
            "5:unresolved:#{effect["name"]}"
          when "opaque_loop"
            "6:opaque_loop:#{effect["loopCid"]}"
          else
            "99:#{effect["kind"]}"
          end
        end

        def primitive_sort(name)
          { "kind" => "primitive", "name" => name }
        end

        def true_formula
          { "kind" => "atomic", "name" => "true", "args" => [] }
        end

        def eq_formula(lhs, rhs)
          { "kind" => "atomic", "name" => "=", "args" => [lhs, rhs] }
        end

        def var(name)
          { "kind" => "var", "name" => name }
        end

        def int_const(value)
          { "kind" => "const", "value" => value.to_i, "sort" => primitive_sort("Int") }
        end

        def bool_const(value)
          { "kind" => "const", "value" => !!value, "sort" => primitive_sort("Bool") }
        end

        def str_const(value)
          { "kind" => "const", "value" => value.to_s, "sort" => primitive_sort("String") }
        end

        def none_const
          { "kind" => "const", "value" => nil, "sort" => primitive_sort("Unit") }
        end

        def ctor(name, *args)
          raise ArgumentError, "operation name must use ruby: namespace: #{name}" unless name.start_with?("ruby:")

          local = name.delete_prefix("ruby:")
          raise ArgumentError, "forbidden Ruby operation name: #{name}" if %w[unknown binop skip].include?(local)

          { "kind" => "ctor", "name" => name, "args" => args }
        end

        def empty_stmt
          ctor("ruby:expr", none_const)
        end

        def seq(first, second)
          ctor("ruby:seq", first, second)
        end

        def fold_seq(statements)
          list = Array(statements)
          return empty_stmt if list.empty?

          list.reduce { |acc, statement| seq(acc, statement) }
        end

        def function_contract(fn_name:, formals:, body_term:, effects:, source_path:, line:)
          {
            "schemaVersion" => "1",
            "kind" => "function-contract",
            "fnName" => fn_name,
            "formals" => formals,
            "formalSorts" => formals.map { primitive_sort("Value") },
            "returnSort" => primitive_sort("Value"),
            "pre" => true_formula,
            "post" => eq_formula(var("return_value"), body_term),
            "bodyCid" => nil,
            "effects" => effects,
            "locus" => { "file" => source_path, "line" => line.to_i, "col" => 1 },
            "autoMintedMementos" => [],
          }
        end

        def source_unit_contract(source_path:, source:, operational_term:)
          {
            "schemaVersion" => "1",
            "kind" => "function-contract",
            "fnName" => "<source-unit:#{source_path}>",
            "formals" => [],
            "formalSorts" => [],
            "returnSort" => primitive_sort("Stmt"),
            "pre" => true_formula,
            "post" => eq_formula(var("return_value"), ctor("ruby:source-unit", str_const(source), operational_term)),
            "bodyCid" => nil,
            "effects" => [],
            "locus" => { "file" => source_path, "line" => 1, "col" => 1 },
            "autoMintedMementos" => [],
          }
        end

        def lift_function(info, source_path, result)
          effects = EffectSet.new
          emitter = Emitter.new(effects: effects)
          body = emitter.statements(info.body)
          function_contract(
            fn_name: info.fn_name,
            formals: info.formals,
            body_term: body,
            effects: effects.sorted,
            source_path: source_path,
            line: info.line,
          )
        rescue UnsupportedSyntax => e
          result.refusals << {
            "kind" => e.kind,
            "function" => info.fn_name,
            "line" => line_of(e.node) || info.line,
            "reason" => e.reason,
          }
          nil
        end

        def parse_params(node)
          return [] if node.nil?

          node = node[1] if node_kind(node) == :paren
          unless node_kind(node) == :params
            raise UnsupportedSyntax.new(node, "unsupported parameter list: #{node_kind(node)}")
          end

          requireds, optionals, rest, posts, keywords, kwrest, block = node[1], node[2], node[3], node[4], node[5], node[6], node[7]
          if optionals || rest || posts || keywords || kwrest || block
            raise UnsupportedSyntax.new(node, "optional/splat/keyword/block parameters are refused")
          end
          Array(requireds).map do |token|
            unless node_kind(token) == :@ident
              raise UnsupportedSyntax.new(token, "non-identifier parameter is refused")
            end
            token_text(token)
          end
        end

        def statements_from_bodystmt(node)
          return [] if node.nil?
          unless node_kind(node) == :bodystmt
            raise UnsupportedSyntax.new(node, "expected bodystmt, got #{node_kind(node)}")
          end

          statements, rescue_part, else_part, ensure_part = node[1], node[2], node[3], node[4]
          if rescue_part || else_part || ensure_part
            raise UnsupportedSyntax.new(node, "begin/rescue/ensure/else bodies are refused")
          end
          Array(statements)
        end

        def node_kind(node)
          node.is_a?(Array) ? node[0] : nil
        end

        def empty_statement?(node)
          node_kind(node) == :void_stmt
        end

        def token_text(node)
          node.is_a?(Array) ? node[1].to_s : node.to_s
        end

        def line_of(node)
          return nil unless node.is_a?(Array)
          if node[0].is_a?(Symbol) && node[0].to_s.start_with?("@") && node[2].is_a?(Array)
            return node[2][0]
          end
          node.each do |child|
            line = line_of(child)
            return line if line
          end
          nil
        end

        def const_like?(node)
          [:const_ref, :const_path_ref, :top_const_ref, :var_ref].include?(node_kind(node))
        end

        def const_name(node)
          case node_kind(node)
          when :const_ref
            token_text(node[1])
          when :top_const_ref
            token_text(node[1])
          when :const_path_ref
            "#{const_name(node[1])}::#{token_text(node[2])}"
          when :var_ref
            token = node[1]
            raise UnsupportedSyntax.new(node, "expected constant receiver") unless node_kind(token) == :@const

            token_text(token)
          else
            raise UnsupportedSyntax.new(node, "unsupported constant path: #{node_kind(node)}")
          end
        end

        def self_receiver?(node)
          node_kind(node) == :var_ref && node_kind(node[1]) == :@kw && token_text(node[1]) == "self"
        end

        def binary_op(op, node)
          BINOPS.fetch(op) { raise UnsupportedSyntax.new(node, "unsupported binary operator: #{op}") }
        end

        def unary_op(op, node)
          UNARYOPS.fetch(op) { raise UnsupportedSyntax.new(node, "unsupported unary operator: #{op}") }
        end

        def io_call?(name)
          IO_CALLS.include?(name) || name.start_with?("File.", "Net::", "TCPSocket.", "UDPSocket.")
        end

        def term_to_source(term)
          Compiler.new.expr(term)
        rescue StandardError
          "<term>"
        end

        def source_unit_contract?(contract)
          term = contract.dig("post", "args", 1)
          term.is_a?(Hash) && term["kind"] == "ctor" && term["name"] == "ruby:source-unit"
        end

        def compile_contract(contract)
          fn_name = contract["fnName"].to_s
          namespace, method, singleton = split_fn_name(fn_name)
          method_name = singleton ? "self.#{method}" : method
          formals = Array(contract["formals"]).join(", ")
          body = Compiler.new.stmt_lines(contract.dig("post", "args", 1))
          body = ["nil"] if body.empty?
          body = body.flat_map { |line| line.to_s.split("\n") }
          lines = ["def #{method_name}(#{formals})"]
          lines.concat(body.map { |line| "  #{line}" })
          lines << "end"
          wrapped = wrap_namespace(lines, namespace)
          wrapped.join("\n") + "\n"
        end

        def split_fn_name(fn_name)
          if fn_name.include?("#")
            namespace, method = fn_name.split("#", 2)
            [namespace, method, false]
          elsif fn_name.include?(".")
            namespace, method = fn_name.rpartition(".").values_at(0, 2)
            [namespace, method, true]
          else
            ["<top>", fn_name, false]
          end
        end

        def wrap_namespace(lines, namespace)
          return lines if namespace.nil? || namespace.empty? || namespace == "<top>"

          parts = namespace.split("::")
          out = []
          parts.each_with_index do |part, index|
            keyword = index == parts.length - 1 ? "class" : "module"
            out << "#{"  " * index}#{keyword} #{part}"
          end
          indent = "  " * parts.length
          out.concat(lines.map { |line| "#{indent}#{line}" })
          parts.length.downto(1) do |depth|
            out << "#{"  " * (depth - 1)}end"
          end
          out
        end

        def relative_to?(path, root)
          path.relative_path_from(root)
          true
        rescue ArgumentError
          false
        end

        def ruby_files(path)
          if path.file?
            return path.extname == ".rb" ? [path] : []
          end
          return [] unless path.directory?

          ignored = Set.new(%w[.git .bundle vendor tmp log node_modules])
          files = []
          Find.find(path.to_s) do |entry|
            base = File.basename(entry)
            if File.directory?(entry) && ignored.include?(base)
              Find.prune
            elsif File.file?(entry) && entry.end_with?(".rb")
              files << Pathname.new(entry)
            end
          end
          files.sort_by(&:to_s)
        end

        def rpc_lift(id, params)
          surface = params["surface"] || SURFACE
          return error(id, 1003, "SURFACE_NOT_SUPPORTED: #{surface}") unless surface == SURFACE

          source_paths = params["source_paths"]
          return error(id, -32602, "source_paths must be a non-empty array") unless source_paths.is_a?(Array) && !source_paths.empty?

          result = lift_paths(params["workspace_root"] || ".", source_paths)
          {
            "jsonrpc" => "2.0",
            "id" => id,
            "result" => {
              "kind" => "ir-document",
              "ir" => result.ir,
              "callEdges" => [],
              "diagnostics" => result.diagnostics,
              "opacityReport" => result.opacity_report,
              "refusals" => result.refusals,
            },
          }
        end

        def rpc_compile(id, params)
          ir = params["ir"]
          return error(id, -32602, "ir must be an array") unless ir.is_a?(Array)

          { "jsonrpc" => "2.0", "id" => id, "result" => { "kind" => "compiled-formula", "body" => compile_ir_document(ir) } }
        end

        def error(id, code, message)
          { "jsonrpc" => "2.0", "id" => id, "error" => { "code" => code, "message" => message } }
        end

        def main(argv)
          if argv.include?("--rpc")
            run_rpc
          else
            warn "Usage: provekit-lift-ruby-source --rpc"
            1
          end
        end
      end

      class Compiler
        BINOPS = {
          "ruby:add" => "+",
          "ruby:sub" => "-",
          "ruby:mul" => "*",
          "ruby:div" => "/",
          "ruby:mod" => "%",
          "ruby:pow" => "**",
          "ruby:eq" => "==",
          "ruby:ne" => "!=",
          "ruby:lt" => "<",
          "ruby:le" => "<=",
          "ruby:gt" => ">",
          "ruby:ge" => ">=",
          "ruby:bitand" => "&",
          "ruby:bitor" => "|",
          "ruby:bitxor" => "^",
          "ruby:shl" => "<<",
          "ruby:shr" => ">>",
        }.freeze

        def stmt_lines(term)
          return [] if term.nil?
          if name(term) == "ruby:seq"
            args = term["args"] || []
            return stmt_lines(args[0]) + stmt_lines(args[1])
          end
          [stmt(term)]
        end

        def stmt(term)
          args = term["args"] || []
          case name(term)
          when "ruby:assign"
            "#{expr(args[0])} = #{expr(args[1])}"
          when "ruby:return"
            "return #{expr(args[0])}"
          when "ruby:if"
            block("if #{expr(args[0])}", stmt_lines(args[1]), "else", stmt_lines(args[2]))
          when "ruby:while"
            block("while #{expr(args[0])}", stmt_lines(args[1]))
          when "ruby:until"
            block("until #{expr(args[0])}", stmt_lines(args[1]))
          when "ruby:for"
            block("for #{expr(args[0])} in #{expr(args[1])}", stmt_lines(args[2]))
          when "ruby:expr"
            expr(args[0])
          when "ruby:raise"
            "raise #{expr(args[0])}"
          else
            expr(term)
          end
        end

        def expr(term)
          return "nil" if term.nil?
          case term["kind"]
          when "const"
            const_expr(term)
          when "var"
            term["name"].to_s
          when "ctor"
            ctor_expr(term)
          else
            raise ArgumentError, "unsupported term kind: #{term["kind"]}"
          end
        end

        private

        def ctor_expr(term)
          args = term["args"] || []
          op = name(term)
          if BINOPS.key?(op)
            return "(#{expr(args[0])} #{BINOPS[op]} #{expr(args[1])})"
          end
          case op
          when "ruby:and"
            "(#{expr(args[0])} && #{expr(args[1])})"
          when "ruby:or"
            "(#{expr(args[0])} || #{expr(args[1])})"
          when "ruby:neg"
            "(-#{expr(args[0])})"
          when "ruby:pos"
            "(+#{expr(args[0])})"
          when "ruby:not"
            "(!#{expr(args[0])})"
          when "ruby:bitnot"
            "(~#{expr(args[0])})"
          when "ruby:ternary"
            "(#{expr(args[0])} ? #{expr(args[1])} : #{expr(args[2])})"
          when "ruby:index"
            "#{expr(args[0])}[#{expr(args[1])}]"
          when "ruby:call"
            callee = const_string(args[0])
            actuals = args[1..] || []
            "#{callee}(#{actuals.map { |arg| expr(arg) }.join(", ")})"
          when "ruby:send"
            receiver = expr(args[0])
            method = const_string(args[1])
            actuals = args[2..] || []
            "#{receiver}.#{method}(#{actuals.map { |arg| expr(arg) }.join(", ")})"
          when "ruby:ivar", "ruby:gvar", "ruby:cvar", "ruby:const"
            const_string(args[0])
          else
            raise ArgumentError, "unsupported Ruby operation in compiler: #{op}"
          end
        end

        def const_expr(term)
          value = term["value"]
          case value
          when nil
            "nil"
          when true
            "true"
          when false
            "false"
          when Integer
            value.to_s
          when String
            value.dump
          else
            raise ArgumentError, "unsupported const value: #{value.inspect}"
          end
        end

        def const_string(term)
          unless term.is_a?(Hash) && term["kind"] == "const" && term["value"].is_a?(String)
            raise ArgumentError, "expected string const: #{term.inspect}"
          end
          term["value"]
        end

        def name(term)
          term.is_a?(Hash) ? term["name"].to_s : ""
        end

        def block(header, body, else_header = nil, else_body = nil)
          lines = [header]
          body = ["nil"] if body.nil? || body.empty?
          lines.concat(body.map { |line| "  #{line}" })
          if else_header
            lines << else_header
            else_body = ["nil"] if else_body.nil? || else_body.empty?
            lines.concat(else_body.map { |line| "  #{line}" })
          end
          lines << "end"
          lines.join("\n")
        end
      end
    end
  end
end
