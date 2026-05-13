# SPDX-License-Identifier: Apache-2.0
#
# Smoke test: protocol conformance of the provekit-lsp-ruby binary.
#
# Asserts:
#   - initialize responds with protocol_version == "provekit-lift/1".
#   - lift returns result.kind == "ir-document" with ir array.
#   - parse (legacy) response has result.declarations as a JSON array, not a string.
#   - parse response has result.callEdges as a JSON array.
#   - Each declaration in a non-empty result is an object with kind=="contract".
#   - Byte-determinism: two runs on the same input produce identical output.
#
# The binary is resolved relative to this test file's location.

require "minitest/autorun"
require "json"
require "open3"

class TestDaemonProtocol < Minitest::Test
  # Path to the binary under test, relative to this file.
  BIN_PATH = File.expand_path("../../bin/provekit-lsp-ruby", __FILE__).freeze

  # Ruby fixture source containing a contract annotation.
  FIXTURE_SOURCE = <<~RUBY.freeze
    # provekit: contract
    def bounded_loop(n)
      (0...n).each { |i| i }
    end
  RUBY

  FIXTURE_PATH = "test_fixture.rb".freeze

  # Build the NDJSON session input for initialize -> parse -> shutdown.
  def build_session(source: FIXTURE_SOURCE, path: FIXTURE_PATH, extra_params: {})
    msgs = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: {} },
      { jsonrpc: "2.0", id: 2, method: "parse",
        params: { path: path, source: source }.merge(extra_params) },
      { jsonrpc: "2.0", id: 3, method: "shutdown" },
    ]
    msgs.map { |m| JSON.generate(m) }.join("\n") + "\n"
  end

  # Spawn the binary, feed ndjson_input on stdin, return parsed response lines.
  def run_lsp(ndjson_input)
    # Use the current Ruby interpreter (guaranteed to be 3.0+ since this test
    # is invoked with the same ruby that passes the re-exec check).
    ruby_bin = RbConfig.ruby
    stdout, stderr, status = Open3.capture3(
      ruby_bin, BIN_PATH, "--rpc",
      stdin_data: ndjson_input,
    )
    assert status.success?,
           "LSP binary exited #{status.exitstatus}; stderr: #{stderr.inspect}"
    stdout.lines.map(&:strip).reject(&:empty?).map { |l| JSON.parse(l) }
  end

  def test_initialize_response
    responses = run_lsp(build_session)
    init_resp = responses.find { |r| r["id"] == 1 }
    assert init_resp, "Expected id 1 not found. Available ids: #{responses.map { |r| r['id'] }}"
    result = init_resp["result"]
    assert_equal "provekit-lsp-ruby", result["name"]
    assert_equal "provekit-lift/1", result["protocol_version"],
                 "expected protocol_version == 'provekit-lift/1'"
    caps = result["capabilities"]
    assert_instance_of Hash, caps, "capabilities should be a Hash"
    assert_includes caps["authoring_surfaces"], "ruby-source"
    assert_equal false, caps["emits_signed_mementos"]
  end

  def test_lift_ir_document_shape
    # Write a fixture file then exercise the lift method.
    require "tmpdir"
    Dir.mktmpdir do |dir|
      fixture_path = File.join(dir, "fixture.rb")
      File.write(fixture_path, FIXTURE_SOURCE)

      msgs = [
        { jsonrpc: "2.0", id: 10, method: "initialize", params: {} },
        { jsonrpc: "2.0", id: 11, method: "lift",
          params: { workspace_root: dir, source_paths: ["fixture.rb"] } },
        { jsonrpc: "2.0", id: 12, method: "shutdown" },
      ]
      ndjson = msgs.map { |m| JSON.generate(m) }.join("\n") + "\n"
      responses = run_lsp(ndjson)

      lift_resp = responses.find { |r| r["id"] == 11 }
      assert lift_resp, "Expected lift id 11 not found. Available ids: #{responses.map { |r| r['id'] }}"
      refute lift_resp["error"], "lift returned error: #{lift_resp.inspect}"
      result = lift_resp["result"]
      assert_equal "ir-document", result["kind"]
      assert_instance_of Array, result["ir"]
      assert_instance_of Array, result["callEdges"]
      assert_equal [], result["callEdges"]
      assert_instance_of Array, result["diagnostics"]
      assert_instance_of Array, result["refusals"]
    end
  end

  def test_lift_ir_contains_contract_entries
    require "tmpdir"
    Dir.mktmpdir do |dir|
      fixture_path = File.join(dir, "fixture.rb")
      File.write(fixture_path, FIXTURE_SOURCE)

      msgs = [
        { jsonrpc: "2.0", id: 10, method: "initialize", params: {} },
        { jsonrpc: "2.0", id: 11, method: "lift",
          params: { workspace_root: dir, source_paths: ["fixture.rb"] } },
        { jsonrpc: "2.0", id: 12, method: "shutdown" },
      ]
      ndjson = msgs.map { |m| JSON.generate(m) }.join("\n") + "\n"
      responses = run_lsp(ndjson)

      lift_resp = responses.find { |r| r["id"] == 11 }
      result = lift_resp["result"]
      ir = result["ir"]
      assert ir.length >= 1, "Expected at least one IR entry from contract-bearing fixture"
      ir.each do |entry|
        assert_equal "contract", entry["kind"], "IR entry kind should be 'contract'"
      end
    end
  end

  def test_parse_declarations_is_array
    responses = run_lsp(build_session)
    parse_resp = responses.find { |r| r["id"] == 2 }
    assert parse_resp, "Expected id 2 not found. Available ids: #{responses.map { |r| r['id'] }}"
    refute_includes parse_resp, "error",
                    "parse returned error: #{parse_resp.inspect}"
    assert_instance_of Array, parse_resp["result"]["declarations"],
                       "declarations should be Array, not #{parse_resp["result"]["declarations"].class}"
  end

  def test_parse_call_edges_is_array
    responses = run_lsp(build_session)
    parse_resp = responses.find { |r| r["id"] == 2 }
    assert parse_resp, "Expected id 2 not found. Available ids: #{responses.map { |r| r['id'] }}"
    assert_instance_of Array, parse_resp["result"]["callEdges"],
                       "callEdges should be Array, not #{parse_resp["result"]["callEdges"].class}"
  end

  def test_declarations_contain_contracts
    responses = run_lsp(build_session)
    parse_resp = responses.find { |r| r["id"] == 2 }
    assert parse_resp, "Expected id 2 not found. Available ids: #{responses.map { |r| r['id'] }}"
    decls = parse_resp["result"]["declarations"]
    assert decls.length >= 1,
           "Expected at least one declaration from contract-bearing fixture"
    decls.each do |d|
      assert_instance_of Hash, d, "declaration is not a Hash: #{d.inspect}"
      assert_equal "contract", d["kind"],
                   "expected kind='contract', got #{d["kind"].inspect}"
    end
  end

  def test_declarations_have_name_field
    responses = run_lsp(build_session)
    parse_resp = responses.find { |r| r["id"] == 2 }
    assert parse_resp, "Expected id 2 not found. Available ids: #{responses.map { |r| r['id'] }}"
    parse_resp["result"]["declarations"].each do |d|
      assert d.key?("name"), "declaration missing 'name': #{d.inspect}"
    end
  end

  def test_empty_source_returns_empty_arrays
    responses = run_lsp(build_session(source: "# no contracts here\n"))
    parse_resp = responses.find { |r| r["id"] == 2 }
    assert parse_resp, "Expected id 2 not found. Available ids: #{responses.map { |r| r['id'] }}"
    result = parse_resp["result"]
    assert_equal [], result["declarations"]
    assert_equal [], result["callEdges"]
  end

  def test_byte_determinism
    ndjson = build_session
    run1 = run_lsp(ndjson)
    run2 = run_lsp(ndjson)
    parse1 = run1.find { |r| r["id"] == 2 }
    assert parse1, "Expected id 2 not found in run1. Available ids: #{run1.map { |r| r['id'] }}"
    parse2 = run2.find { |r| r["id"] == 2 }
    assert parse2, "Expected id 2 not found in run2. Available ids: #{run2.map { |r| r['id'] }}"
    assert_equal JSON.generate(parse1.sort.to_h),
                 JSON.generate(parse2.sort.to_h),
                 "parse response is not byte-deterministic across two runs"
  end

  def test_unsupported_language_returns_error
    responses = run_lsp(build_session(source: "fn foo() {}", extra_params: { "language" => "go" }))
    parse_resp = responses.find { |r| r["id"] == 2 }
    assert parse_resp, "Expected id 2 not found. Available ids: #{responses.map { |r| r['id'] }}"
    assert parse_resp.key?("error"),
           "Expected error for unsupported language, got: #{parse_resp.inspect}"
    assert_equal(-32602, parse_resp["error"]["code"])
  end
end
