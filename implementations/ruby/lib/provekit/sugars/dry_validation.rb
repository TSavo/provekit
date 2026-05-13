module Provekit
  module Sugars
    module DryValidation
      CID = "provekit:sugar:ruby:dry-validation:v1"

      def self.patterns
        [
          {
            proofir: "required(field) and sort(field, String)",
            ruby: lambda { |field| "required(:#{field}).filled(:string)" },
            inverse: lambda { |node| node.to_s.include?("required") ? "required(field)" : nil },
            role: "require",
            cid: CID
          },
          {
            proofir: "gte(field, n)",
            ruby: lambda { |field, n| "required(:#{field}).filled(:integer, gteq?: #{n})" },
            inverse: lambda { |node| node.to_s.include?("gteq?") ? "gte(field, n)" : nil },
            role: "pre",
            cid: CID
          }
        ]
      end
    end
  end
end
