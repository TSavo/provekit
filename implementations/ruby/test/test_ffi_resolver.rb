# SPDX-License-Identifier: Apache-2.0
#
# Tests for Provekit::Lift::FfiResolver — Ruby Fiddle + ffi gem resolver.
# spec #114 R3, extends #127 (Go cgo), #131 (Python ctypes), #132 (Java JNI).

require "minitest/autorun"
require_relative "../lib/provekit/ir"
require_relative "../lib/provekit/lift/ffi_resolver"

class TestFfiResolver < Minitest::Test
  Resolver = Provekit::Lift::FfiResolver

  # ── Pattern A: Fiddle + dlload + extern ─────────────────────────────────

  FIDDLE_SOURCE = <<~RUBY
    require 'fiddle'
    require 'fiddle/import'
    module RustBindings
      extend Fiddle::Importer
      dlload "librust_callee.so"
      extern "int process(int)"
    end
    def caller_fn(value)
      RustBindings.process(value)
    end
  RUBY

  def test_fiddle_pattern_emits_call_edge
    result = Resolver.resolve(FIDDLE_SOURCE, path: "test.rb")
    assert_equal 1, result.call_edges.length, "expected one call-edge for Fiddle pattern"
    edge = result.call_edges.first
    assert_equal "rust-kit:process", edge.target_symbol
    assert_nil edge.target_contract_cid
    assert_equal "test.rb", edge.call_site_file
    assert_empty result.linker_errors
  end

  # ── Pattern B: ffi gem, single-name attach_function ─────────────────────

  FFI_SINGLE_SOURCE = <<~RUBY
    require 'ffi'
    module RustBindings
      extend FFI::Library
      ffi_lib 'librust_callee'
      attach_function :process, [:int], :int
    end
    def caller_fn(value)
      RustBindings.process(value)
    end
  RUBY

  def test_ffi_gem_single_name_emits_call_edge
    result = Resolver.resolve(FFI_SINGLE_SOURCE, path: "test.rb")
    assert_equal 1, result.call_edges.length, "expected one call-edge for ffi gem pattern"
    edge = result.call_edges.first
    assert_equal "rust-kit:process", edge.target_symbol
    assert_nil edge.target_contract_cid
    assert_empty result.linker_errors
  end

  # ── Pattern C: ffi gem, renamed binding ──────────────────────────────────

  FFI_RENAMED_SOURCE = <<~RUBY
    require 'ffi'
    module Foo
      extend FFI::Library
      ffi_lib 'rust_callee'
      attach_function :rust_proc, :process, [:int], :int
    end
    def caller_fn(n)
      Foo.rust_proc(n)
    end
  RUBY

  def test_ffi_gem_renamed_binding_uses_native_name
    result = Resolver.resolve(FFI_RENAMED_SOURCE, path: "test.rb")
    assert_equal 1, result.call_edges.length, "expected one call-edge for renamed binding"
    edge = result.call_edges.first
    # targetSymbol must use the NATIVE name (process), not the ruby alias (rust_proc)
    assert_equal "rust-kit:process", edge.target_symbol,
      "renamed binding must emit native name, not ruby alias"
    assert_empty result.linker_errors
  end

  # ── Test 4: file with no FFI imports -> no FFI call-edges ───────────────

  NO_FFI_SOURCE = <<~RUBY
    def greet(name)
      "Hello, \#{name}"
    end
    greet("world")
  RUBY

  def test_no_ffi_imports_produces_no_edges
    result = Resolver.resolve(NO_FFI_SOURCE, path: "test.rb")
    assert_empty result.call_edges, "non-FFI source must produce no call-edges"
    assert_empty result.linker_errors
  end

  # ── Test 5: unknown library -> resolver-error:<name> ────────────────────
  # An ffi_lib with an unresolvable name. Because resolve_kit returns nil
  # only for empty strings (all non-empty non-system names -> cpp-kit per the
  # Java convention), we test with an explicitly empty lib name.
  # Per spec #97 R2: empty/unknown must produce "resolver-error:<name>".

  FFI_UNKNOWN_LIB_SOURCE = <<~RUBY
    require 'ffi'
    module Unknown
      extend FFI::Library
      ffi_lib ''
      attach_function :do_thing, [:int], :int
    end
    def caller_fn(n)
      Unknown.do_thing(n)
    end
  RUBY

  def test_unknown_library_emits_resolver_error
    result = Resolver.resolve(FFI_UNKNOWN_LIB_SOURCE, path: "test.rb")
    # With empty lib name, resolve_kit returns nil -> linker-error path
    assert_empty result.call_edges, "unknown lib must produce no valid call-edges"
    assert_equal 1, result.linker_errors.length, "unknown lib must produce a linker-error"
    err = result.linker_errors.first
    assert_equal "linker-error", err[:kind]
    assert_equal "unresolvable-ffi-target", err[:errorKind]
  end

  # ── Test 6: byte-determinism ─────────────────────────────────────────────

  def test_byte_determinism
    r1 = Resolver.resolve(FFI_RENAMED_SOURCE, path: "test.rb")
    r2 = Resolver.resolve(FFI_RENAMED_SOURCE, path: "test.rb")

    edges1 = r1.call_edges.map { |e| [e.target_symbol, e.call_site_line, e.call_site_column] }
    edges2 = r2.call_edges.map { |e| [e.target_symbol, e.call_site_line, e.call_site_column] }
    assert_equal edges1, edges2, "two runs over same source must produce identical call-edge stream"
  end

  # ── Helpers: strip_lib_name / resolve_kit ────────────────────────────────

  def test_strip_lib_name_various
    assert_equal "rust_callee", Resolver.strip_lib_name("librust_callee.so")
    assert_equal "rust_callee", Resolver.strip_lib_name("librust_callee")
    assert_equal "foo",         Resolver.strip_lib_name("foo.dll")
    assert_equal "foo",         Resolver.strip_lib_name("libfoo.dylib")
    assert_equal "c",           Resolver.strip_lib_name("libc.so.6")
  end

  def test_resolve_kit_rust_prefix
    assert_equal "rust-kit",    Resolver.resolve_kit("rust_callee")
    assert_equal "rust-kit",    Resolver.resolve_kit("rustffi")
    assert_equal "libc-system", Resolver.resolve_kit("c")
    assert_equal "libc-system", Resolver.resolve_kit("pthread")
    assert_equal "cpp-kit",     Resolver.resolve_kit("mylib")
    assert_nil                  Resolver.resolve_kit("")
    assert_nil                  Resolver.resolve_kit(nil)
  end
end
