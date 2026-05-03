# SPDX-License-Identifier: Apache-2.0
#
# LSP plugin round-trip test (#221).
#
# Spawns `bin/provekit-lsp-ruby --rpc` as a subprocess and drives the
# NDJSON-over-stdio plugin protocol end to end:
#
#   1. initialize -> name/version/capabilities
#   2. parse      -> declarations + warnings
#   3. shutdown   -> null result, clean exit
#
# This is the single test that proves the binary actually speaks the protocol.

require "minitest/autorun"
require "json"
require "open3"

class TestLspRoundTrip < Minitest::Test
  BIN = File.expand_path("../bin/provekit-lsp-ruby", __dir__)

  def spawn_plugin
    Open3.popen3("ruby", BIN, "--rpc")
  end

  def exchange(stdin, stdout, payload)
    stdin.write(JSON.generate(payload) + "\n")
    stdin.flush
    line = stdout.gets
    refute_nil line, "plugin closed stdout"
    JSON.parse(line)
  end

  def test_round_trip
    stdin, stdout, stderr, wait = spawn_plugin
    begin
      # 1. initialize
      init = exchange(stdin, stdout,
        { jsonrpc: "2.0", id: 1, method: "initialize", params: {} })
      assert_equal "2.0", init["jsonrpc"]
      assert_equal 1, init["id"]
      refute_nil init["result"], "initialize returned error: #{init.inspect}"
      assert_equal "provekit-lsp-ruby", init["result"]["name"]
      assert_kind_of String, init["result"]["version"]
      assert_includes init["result"]["capabilities"], "parse"

      # 2. parse
      sample = <<~RUBY
        # //provekit:contract
        def add(a, b)
          a + b
        end
      RUBY
      parse = exchange(stdin, stdout,
        { jsonrpc: "2.0", id: 2, method: "parse",
          params: { path: "sample.rb", source: sample } })
      assert_equal 2, parse["id"]
      refute_nil parse["result"], "parse returned error: #{parse.inspect}"
      assert parse["result"].key?("declarations"),
        "parse result missing `declarations`: #{parse.inspect}"
      assert parse["result"].key?("warnings"),
        "parse result missing `warnings`: #{parse.inspect}"
      assert_kind_of Array, parse["result"]["warnings"]

      # 3. shutdown
      shut = exchange(stdin, stdout,
        { jsonrpc: "2.0", id: 3, method: "shutdown" })
      assert_equal 3, shut["id"]
      assert_nil shut["result"]

      # Plugin must exit cleanly within a reasonable window.
      stdin.close rescue nil
      Timeout.timeout(5) { wait.value }
      assert wait.value.success?, "plugin exited with #{wait.value.inspect}"
    ensure
      [stdin, stdout, stderr].each { |io| io.close rescue nil }
      begin
        Process.kill("KILL", wait.pid) if wait.alive?
      rescue Errno::ESRCH, Errno::ECHILD
      end
    end
  end

  def test_unknown_method_returns_jsonrpc_error
    stdin, stdout, stderr, wait = spawn_plugin
    begin
      exchange(stdin, stdout,
        { jsonrpc: "2.0", id: 1, method: "initialize", params: {} })
      bad = exchange(stdin, stdout,
        { jsonrpc: "2.0", id: 2, method: "no_such_method" })
      refute_nil bad["error"]
      assert_equal(-32601, bad["error"]["code"])

      # Plugin still responds after an error.
      shut = exchange(stdin, stdout,
        { jsonrpc: "2.0", id: 3, method: "shutdown" })
      assert_nil shut["result"]
      stdin.close rescue nil
      Timeout.timeout(5) { wait.value }
    ensure
      [stdin, stdout, stderr].each { |io| io.close rescue nil }
      begin
        Process.kill("KILL", wait.pid) if wait.alive?
      rescue Errno::ESRCH, Errno::ECHILD
      end
    end
  end
end

require "timeout"
