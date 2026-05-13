require "json"
require "provekit/ruby_lifter"
require "provekit/ruby_realizer"
require "provekit/sugar_dict"
require "provekit/version"

module Provekit
  class PepServer
    MANIFEST = {
      "name" => "provekit-ruby",
      "version" => VERSION,
      "protocol_version" => "provekit-plugin/1.7.0",
      "kit_cid" => "blake3-512:df3bd324caec5f9e818bb2f0a8d44ac923a9136f923a946f525023001637832a21a21ac32145c5c27de71c3f429e9ae6cf2d5f1a163d654df717d620169b9322",
      "language" => "ruby",
      "supported_verbs" => ["lift_source", "realize_concept"],
      "supported_sugar_cids" => SugarDict.default_cids,
      "capabilities" => {
        "authoring_surfaces" => ["ruby-source", "ruby-sorbet", "ruby-comments"],
        "ir_version" => "v1.1.0",
        "emits_signed_mementos" => false
      }
    }.freeze

    def initialize(input = STDIN, output = STDOUT)
      @input = input
      @output = output
      @lifter = RubyLifter.new
      @realizer = RubyRealizer.new
    end

    def run
      @input.each_line do |line|
        next if line.strip.empty?
        handle(JSON.parse(line))
      rescue JSON::ParserError
        write_error(nil, -32700, "parse error")
      rescue => e
        write_error(nil, -32603, e.message)
      end
    end

    def handle(request)
      id = request["id"]
      case request["method"]
      when "provekit.plugin.describe"
        write_result(id, MANIFEST)
      when "provekit.plugin.invoke"
        write_result(id, invoke(request["params"] || {}))
      when "provekit.plugin.shutdown"
        write_result(id, nil)
        exit 0
      when "initialize"
        write_result(id, MANIFEST)
      when "lift"
        params = request["params"] || {}
        source = source_from_lift_params(params)
        write_result(id, {
          "kind" => "ir-document",
          "ir" => @lifter.lift_source(source, params["path"] || "source.rb")["declarations"],
          "diagnostics" => []
        })
      when "shutdown"
        write_result(id, nil)
        exit 0
      else
        write_error(id, -32601, "unknown method: #{request["method"]}")
      end
    end

    def invoke(params)
      verb = params["verb"] || params["name"]
      payload = params["params"] || params["payload"] || {}
      case verb
      when "lift_source"
        @lifter.lift_source(payload["source"].to_s, payload["path"] || "source.rb")
      when "realize_concept"
        @realizer.realize(payload["proofir"] || payload["ir"] || {}, payload["plan"] || {})
      else
        raise ArgumentError, "unsupported verb: #{verb}"
      end
    end

    private

    def source_from_lift_params(params)
      return params["source"] if params["source"]
      paths = Array(params["source_paths"])
      return "" if paths.empty?
      root = params["workspace_root"] || "."
      paths.map { |path| File.read(File.expand_path(path, root)) }.join("\n")
    end

    def write_result(id, result)
      @output.puts(JSON.generate("jsonrpc" => "2.0", "id" => id, "result" => result))
      @output.flush
    end

    def write_error(id, code, message)
      @output.puts(JSON.generate("jsonrpc" => "2.0", "id" => id, "error" => { "code" => code, "message" => message }))
      @output.flush
    end
  end
end
