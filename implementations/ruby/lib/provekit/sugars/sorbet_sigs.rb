module Provekit
  module Sugars
    module SorbetSigs
      CID = "provekit:sugar:ruby:sorbet-sigs:v1"

      def self.patterns
        [
          {
            proofir: "forall x. eq(x, nil) => false",
            ruby: lambda { |var| "T.must(#{var})" },
            inverse: lambda { |node| t_call(node, "must") ? "forall #{arg_name(node)}. eq(#{arg_name(node)}, nil) => false" : nil },
            role: "require",
            cid: CID
          },
          {
            proofir: "typed_binding(name, sort)",
            ruby: lambda { |name, value, type| "T.let(#{value}, #{type})" },
            inverse: lambda { |node| t_call(node, "let") ? "typed_binding" : nil },
            role: "witness",
            cid: CID
          },
          {
            proofir: "function(params, returns)",
            ruby: lambda { |params, returns_type| "sig { params(#{params}).returns(#{returns_type}) }" },
            inverse: lambda { |node| node.to_s.include?("sig") ? "function(params, returns)" : nil },
            role: "pre-post",
            cid: CID
          }
        ]
      end

      def self.t_call(node, method_name)
        node.to_s.include?("T.#{method_name}")
      end

      def self.arg_name(node)
        node.to_s[/\(([^)]+)\)/, 1] || "x"
      end
    end
  end
end
