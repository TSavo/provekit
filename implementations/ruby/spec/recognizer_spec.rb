$LOAD_PATH.unshift File.expand_path("../lib", __dir__)

require "tmpdir"
require "fileutils"

require "provekit/lift/ruby_source"
require "provekit/ruby_lifter"
require "provekit/ruby_recognizer"
require "provekit/sugars/sorbet_sigs"

describe Provekit::RubyRecognizer do
  def binding_from(source, concept = "concept:http-request")
    entry = Provekit::RubyLifter.new.lift_source(source, "shim.rb")["declarations"].first
    {
      "concept_name" => concept,
      "library_tag" => "ruby-http-client",
      "family" => "concept:family:http",
      "ast_template" => entry["body_source"]["ast_template"],
      "template_cid" => entry["body_source"]["template_cid"],
      "param_names" => entry["body_source"]["param_names"],
      "contract_cid" => "blake3-512:" + ("c" * 128)
    }
  end

  it "recognizes an exact alpha-equivalent Ruby sugar body" do
    shim = <<~RUBY
      def fetch_url(url, headers)
        client.execute(url, headers)
      end
    RUBY
    user = <<~RUBY
      def send_request(uri, h)
        client.execute(uri, h)
      end
    RUBY

    response = Provekit::RubyRecognizer.recognize_text(user, "src/user.rb", [binding_from(shim)])
    tag = response["tags"].first

    expect(response["tags"].length).to eq(1)
    expect(tag["file"]).to eq("src/user.rb")
    expect(tag["function_name"]).to eq("send_request")
    expect(tag["concept_name"]).to eq("concept:http-request")
    expect(tag["library_tag"]).to eq("ruby-http-client")
    expect(tag["family"]).to eq("concept:family:http")
    expect(tag["match_tier"]).to eq("exact")
    expect(tag["param_bindings"]).to eq([
      { "index" => 1, "source_text" => "uri" },
      { "index" => 2, "source_text" => "h" }
    ])
  end

  it "does not recognize a different Ruby body" do
    shim = <<~RUBY
      def fetch_url(url, headers)
        client.execute(url, headers)
      end
    RUBY
    user = <<~RUBY
      def send_request(uri, h)
        client.send(uri, h)
      end
    RUBY

    response = Provekit::RubyRecognizer.recognize_text(user, "src/user.rb", [binding_from(shim)])

    expect(response["tags"]).to eq([])
  end

  it "resolves Ruby-owned sugar templates on the RPC recognize path" do
    Dir.mktmpdir("provekit-ruby-recognize") do |root|
      FileUtils.mkdir_p(File.join(root, "src"))
      File.write(File.join(root, "src", "user.rb"), <<~RUBY)
        def require_name(name)
          T.must(name)
        end
      RUBY

      response = Provekit::Lift::RubySource.dispatch(
        "jsonrpc" => "2.0",
        "id" => 1,
        "method" => "provekit.plugin.recognize",
        "params" => {
          "project_root" => root,
          "source_paths" => ["src/user.rb"]
        }
      )

      expect(response["result"]["tags"].length).to eq(1)
      tag = response["result"]["tags"].first
      expect(tag["function_name"]).to eq("require_name")
      expect(tag["concept_name"]).to eq("forall x. eq(x, nil) => false")
      expect(tag["library_tag"]).to eq(Provekit::Sugars::SorbetSigs::CID)
      expect(tag["template_cid"].start_with?("blake3-512:")).to eq(true)
    end
  end
end
