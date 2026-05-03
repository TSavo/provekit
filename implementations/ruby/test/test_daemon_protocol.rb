# SPDX-License-Identifier: Apache-2.0
#
# Smoke test: protocol conformance of the provekit-lsp-ruby binary.
#
# Asserts:
#   - The binary responds to initialize/parse/shutdown.
#   - parse response has result.declarations as a JSON array, not a string.
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
    result = init_resp["result"]
    assert_equal "provekit-lsp-ruby", result["name"]
    assert_includes result["capabilities"], "parse"
  end

  def test_parse_declarations_is_array
    responses = run_lsp(build_session)
    parse_resp = responses.find { |r| r["id"] == 2 }
    refute_includes parse_resp, "error",
                    "parse returned error: #{parse_resp.inspect}"
    assert_instance_of Array, parse_resp["result"]["declarations"],
                       "declarations should be Array, not #{parse_resp["result"]["declarations"].class}"
  end

  def test_parse_call_edges_is_array
    responses = run_lsp(build_session)
    parse_resp = responses.find { |r| r["id"] == 2 }
    assert_instance_of Array, parse_resp["result"]["callEdges"],
                       "callEdges should be Array, not #{parse_resp["result"]["callEdges"].class}"
  end

  def test_declarations_contain_contracts
    responses = run_lsp(build_session)
    parse_resp = responses.find { |r| r["id"] == 2 }
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
    parse_resp["result"]["declarations"].each do |d|
      assert d.key?("name"), "declaration missing 'name': #{d.inspect}"
    end
  end

  def test_empty_source_returns_empty_arrays
    responses = run_lsp(build_session(source: "# no contracts here\n"))
    parse_resp = responses.find { |r| r["id"] == 2 }
    result = parse_resp["result"]
    assert_equal [], result["declarations"]
    assert_equal [], result["callEdges"]
  end

  def test_byte_determinism
    ndjson = build_session
    run1 = run_lsp(ndjson)
    run2 = run_lsp(ndjson)
    parse1 = run1.find { |r| r["id"] == 2 }
    parse2 = run2.find { |r| r["id"] == 2 }
    assert_equal JSON.generate(parse1.sort.to_h),
                 JSON.generate(parse2.sort.to_h),
                 "parse response is not byte-deterministic across two runs"
  end

  def test_unsupported_language_returns_error
    responses = run_lsp(build_session(source: "fn foo() {}", extra_params: { "language" => "go" }))
    parse_resp = responses.find { |r| r["id"] == 2 }
    assert parse_resp.key?("error"),
           "Expected error for unsupported language, got: #{parse_resp.inspect}"
    assert_equal(-32602, parse_resp["error"]["code"])
  end
end
