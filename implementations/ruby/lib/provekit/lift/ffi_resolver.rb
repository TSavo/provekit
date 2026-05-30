# SPDX-License-Identifier: Apache-2.0
#
# ffi_resolver: Ruby Fiddle + ffi gem FFI call-site resolver.
#
# Walks Ruby source via RubyVM::AbstractSyntaxTree and detects FFI load +
# call patterns, emitting CallEdgeDecl mementos per
# protocol/specs/2026-05-03-bridge-linkage-protocol.md §1 R1 and R3.
#
# Mirrors Go's cgo resolver (PR #127), Python's ctypes resolver (PR #131),
# and Java's JNI resolver (PR #132). Each FFI host has its own resolver;
# the IR is the common substrate beneath them all.
#
# Supported patterns (spec #114 R3, Ruby):
#
#   Pattern A: Fiddle (Ruby stdlib)
#     require 'fiddle'
#     require 'fiddle/import'
#     module RustBindings
#       extend Fiddle::Importer
#       dlload "librust_callee.so"
#       extern "int process(int)"
#     end
#     RustBindings.process(value)   # call site
#
#   Pattern B: ffi gem (single-name attach_function)
#     require 'ffi'
#     module RustBindings
#       extend FFI::Library
#       ffi_lib 'librust_callee'
#       attach_function :process, [:int], :int
#     end
#     RustBindings.process(value)   # call site
#
#   Pattern C: ffi gem with renamed binding
#     module Foo
#       extend FFI::Library
#       ffi_lib 'rust_callee'
#       attach_function :rust_proc, :process, [:int], :int
#     end
#     Foo.rust_proc(n)              # call site -> targetSymbol = "rust-kit:process"
#
# Out of scope: RubyInline, extconf.rb C extensions, dynamic dlopen via
# instance_variable_get / runtime strings. Follow-up resolvers.

require "provekit/ir"

module Provekit
  module Lift
    module FfiResolver
      # ── System library names (pseudo-kit "libc-system") ──────────────────

      SYSTEM_LIBS = %w[
        c m pthread dl rt util resolv nsl z bz2 lzma crypto ssl X11 Xext GL
      ].to_set.freeze

      # ── Library-name normalisation ────────────────────────────────────────

      # Strip directory prefix, "lib" prefix, and .so/.dll/.dylib suffix.
      #
      # Examples:
      #   "librust_callee.so"  -> "rust_callee"
      #   "librust_callee"     -> "rust_callee"
      #   "./libfoo.dylib"     -> "foo"
      #   "foo.dll"            -> "foo"
      #   "libc.so.6"          -> "c"
      def self.strip_lib_name(raw)
        name = File.basename(raw.to_s)

        # Strip known extensions repeatedly (handles libc.so.6).
        loop do
          ext = File.extname(name).downcase
          if %w[.so .dll .dylib .a].include?(ext)
            name = File.basename(name, ext)
          elsif ext =~ /\A\.\d+\z/
            # Trailing version component like .6 in libc.so.6
            name = File.basename(name, ext)
          else
            break
          end
        end

        # Strip 'lib' prefix.
        name = name[3..] if name.start_with?("lib") && name.length > 3

        name
      end

      # ── Kit resolution ────────────────────────────────────────────────────

      # Map a normalised library name to a kit prefix.
      #
      # Resolution order (matches Java JniResolver, PR #132):
      #   1. "rust" prefix (case-insensitive) -> "rust-kit"
      #   2. Known system libs               -> "libc-system"
      #   3. Any other non-empty name        -> "cpp-kit"
      #   4. Empty / unknown                 -> nil (caller emits resolver-error)
      def self.resolve_kit(lib_name)
        return nil if lib_name.nil? || lib_name.empty?

        if lib_name.downcase.start_with?("rust")
          "rust-kit"
        elsif SYSTEM_LIBS.include?(lib_name)
          "libc-system"
        else
          "cpp-kit"
        end
      end

      # ── AST-level helpers ─────────────────────────────────────────────────

      # Return the constant-path string for a const node, e.g. [:FFI, :Library]
      # -> "FFI::Library", or nil if not a const/colon2 node.
      def self.const_path(node)
        return nil unless node
        case node.type
        when :CONST
          node.children.first.to_s
        when :COLON2
          left, right = node.children
          left_str = const_path(left)
          left_str ? "#{left_str}::#{right}" : right.to_s
        else
          nil
        end
      end

      # Extract a string literal value from a node, or nil.
      def self.string_value(node)
        return nil unless node
        return node.children.first if node.type == :STR
        # Interpolated string with single literal segment
        if node.type == :DSTR
          parts = node.children.compact
          return parts.first.children.first if parts.length == 1 && parts.first.type == :STR
        end
        nil
      end

      # Extract a symbol value from a :LIT or :SYM node, or nil.
      def self.symbol_value(node)
        return nil unless node
        return node.children.first if node.type == :LIT && node.children.first.is_a?(Symbol)
        return node.children.first if node.type == :SYM
        nil
      end

      # Collect all nodes of a given type by walking an AST node recursively.
      def self.collect_nodes(node, *types)
        return [] unless node.respond_to?(:children)
        results = []
        results << node if types.include?(node.type)
        node.children.each do |child|
          results.concat(collect_nodes(child, *types)) if child.is_a?(RubyVM::AbstractSyntaxTree::Node)
        end
        results
      end

      # ── Module scanning ───────────────────────────────────────────────────

      # A registered FFI module: maps ruby method name -> (kit, native_name).
      ModuleBinding = Struct.new(:module_name, :ruby_name, :native_name, :kit, keyword_init: true)

      # Scan the AST for module definitions that extend Fiddle::Importer or
      # FFI::Library. Return a list of ModuleBinding records.
      def self.scan_ffi_modules(root)
        bindings = []
        collect_nodes(root, :MODULE).each do |mod_node|
          module_name_node, body = mod_node.children
          mod_name = const_path(module_name_node) || "unknown"

          next unless body

          # Detect extend calls within the module.
          extend_calls = collect_nodes(body, :FCALL, :VCALL).select do |n|
            method_sym = n.children.first
            method_sym.to_s == "extend"
          end
          # Also cover :CALL nodes: ModuleName.extend(...)
          extend_calls += collect_nodes(body, :CALL).select do |n|
            n.children[1].to_s == "extend"
          end

          is_fiddle = false
          is_ffi    = false

          extend_calls.each do |ecall|
            # The argument to extend is in the args array
            args_node = ecall.children.last
            next unless args_node

            arg_nodes = case args_node.type
                        when :LIST, :ARRAY then args_node.children.compact
                        when :ARGSPUSH, :ARGS_ADD_BLOCK then [args_node.children.first].compact
                        else [args_node]
                        end

            arg_nodes.each do |arg|
              path = const_path(arg)
              is_fiddle = true if path == "Fiddle::Importer"
              is_ffi    = true if path == "FFI::Library"
            end
          end

          next unless is_fiddle || is_ffi

          # Find the library load.
          lib_name = nil
          if is_fiddle
            # dlload "libfoo.so"
            collect_nodes(body, :FCALL).each do |fcall|
              next unless fcall.children.first.to_s == "dlload"
              args_node = fcall.children.last
              next unless args_node
              arg = args_node.is_a?(RubyVM::AbstractSyntaxTree::Node) ? args_node.children.compact.first : nil
              arg ||= args_node if args_node.is_a?(RubyVM::AbstractSyntaxTree::Node)
              raw = string_value(arg)
              lib_name = strip_lib_name(raw) if raw
            end

            # extern "int process(int)": extract function name from the C sig.
            collect_nodes(body, :FCALL).each do |fcall|
              next unless fcall.children.first.to_s == "extern"
              args_node = fcall.children.last
              next unless args_node
              sig_node = args_node.is_a?(RubyVM::AbstractSyntaxTree::Node) ? args_node.children.compact.first : nil
              sig = string_value(sig_node)
              next unless sig
              # Parse "int process(int)" -> extract function name
              if sig =~ /\b(\w+)\s*\(/
                native_name = $1
                kit = resolve_kit(lib_name || "")
                target_sym = kit ? "#{kit}:#{native_name}" : "resolver-error:#{native_name}"
                bindings << ModuleBinding.new(
                  module_name: mod_name,
                  ruby_name: native_name,
                  native_name: native_name,
                  kit: kit,
                )
              end
            end
          end

          if is_ffi
            # ffi_lib 'libfoo' or ffi_lib 'foo'
            collect_nodes(body, :FCALL).each do |fcall|
              next unless fcall.children.first.to_s == "ffi_lib"
              args_node = fcall.children.last
              next unless args_node
              arg = args_node.is_a?(RubyVM::AbstractSyntaxTree::Node) ? args_node.children.compact.first : nil
              raw = string_value(arg)
              lib_name = strip_lib_name(raw) if raw
            end

            kit = resolve_kit(lib_name || "")

            # attach_function :ruby_name, :native_name, [...], ret
            # attach_function :ruby_name, [...], ret  (ruby_name == native_name)
            collect_nodes(body, :FCALL).each do |fcall|
              next unless fcall.children.first.to_s == "attach_function"
              args_node = fcall.children.last
              next unless args_node
              args = args_node.is_a?(RubyVM::AbstractSyntaxTree::Node) ? args_node.children.compact : []

              ruby_name = symbol_value(args[0])&.to_s
              next unless ruby_name

              # Determine native_name: if args[1] is a symbol, it's the native name;
              # if args[1] is an array literal (ARRAY/LIST), ruby_name == native_name.
              native_name = if args[1] && %i[LIT SYM].include?(args[1].type) && symbol_value(args[1])
                              symbol_value(args[1]).to_s
                            else
                              ruby_name
                            end

              bindings << ModuleBinding.new(
                module_name: mod_name,
                ruby_name: ruby_name,
                native_name: native_name,
                kit: kit,
              )
            end
          end
        end
        bindings
      end

      # ── Call-site scanning ────────────────────────────────────────────────

      # Walk the AST for call sites of the form Module.method_name(...) where
      # the module + method_name is a registered FFI binding.
      # Returns an array of hashes: {binding:, line:, column:}.
      def self.scan_call_sites(root, bindings)
        # Build a lookup: "ModuleName#ruby_name" -> ModuleBinding
        lookup = {}
        bindings.each do |b|
          lookup["#{b.module_name}##{b.ruby_name}"] = b
        end
        return [] if lookup.empty?

        results = []
        collect_nodes(root, :CALL).each do |call_node|
          recv_node, method_sym, _args = call_node.children
          method_name = method_sym.to_s
          recv_name   = const_path(recv_node)
          next unless recv_name

          key = "#{recv_name}##{method_name}"
          b = lookup[key]
          next unless b

          line   = call_node.first_lineno
          column = call_node.first_column

          results << { binding: b, line: line, column: column }
        end
        results
      end

      def self.function_ranges(root)
        collect_nodes(root, :DEF, :DEFN, :DEFS).map do |def_node|
          name = def_node.children.first.to_s
          first_line = def_node.first_lineno
          last_line = if def_node.respond_to?(:last_lineno)
                        def_node.last_lineno
                      else
                        first_line
                      end
          [name, first_line, last_line || first_line]
        end
      end

      def self.enclosing_function_name(ranges, line)
        ranges
          .select { |_name, first_line, last_line| line >= first_line && line <= last_line }
          .max_by { |_name, first_line, _last_line| first_line }
          &.first
      end

      # ── Public API ────────────────────────────────────────────────────────

      FfiResolverResult = Struct.new(:call_edges, :linker_errors, keyword_init: true)

      # Resolve FFI call sites in Ruby source.
      #
      # @param source [String] Ruby source text.
      # @param path [String] Source file path (for locus field).
      # @param contract_index [Hash<String, String>] function-name -> contractCid.
      #   Only call sites within contracted functions emit edges (same guard as
      #   Go and Python resolvers).
      # @return [FfiResolverResult] with call_edges and linker_errors arrays.
      #
      # The two lists are emitted in deterministic source order (line asc,
      # column asc within the same line) so two runs over byte-identical source
      # produce byte-identical output (spec #114 R5).
      def self.resolve(source, path:, contract_index: {})
        begin
          root = RubyVM::AbstractSyntaxTree.parse(source)
        rescue SyntaxError, ArgumentError
          return FfiResolverResult.new(call_edges: [], linker_errors: [])
        end

        bindings = scan_ffi_modules(root)
        return FfiResolverResult.new(call_edges: [], linker_errors: []) if bindings.empty?

        call_sites = scan_call_sites(root, bindings)
        ranges = function_ranges(root)

        call_edges    = []
        linker_errors = []

        call_sites.sort_by { |cs| [cs[:line], cs[:column]] }.each do |cs|
          b    = cs[:binding]
          line = cs[:line]
          col  = cs[:column]

          evidence = IR.atomic("call-site-obligation", IR.var(name: b.module_name))

          if b.kit.nil?
            linker_errors << {
              kind: "linker-error",
              errorKind: "unresolvable-ffi-target",
              libName: b.native_name,
              callSiteFile: path,
              callSiteLine: line,
              callSiteColumn: col,
            }
          else
            target_symbol = "#{b.kit}:#{b.native_name}"
            caller_name = enclosing_function_name(ranges, line)
            source_contract_cid =
              if caller_name && contract_index[caller_name]
                contract_index[caller_name]
              elsif caller_name
                "pending-ruby:#{caller_name}"
              end

            call_edges << IR::CallEdgeDecl.new(
              source_contract_cid: source_contract_cid,
              target_contract_cid: nil,
              target_symbol: target_symbol,
              call_site_file: path,
              call_site_line: line,
              call_site_column: col,
              evidence_term: evidence,
            )
          end
        end

        FfiResolverResult.new(call_edges: call_edges, linker_errors: linker_errors)
      end
    end
  end
end
