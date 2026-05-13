module Provekit
  module Sugars
    module CommentRoles
      CID = "provekit:sugar:ruby:comment-roles:v1"

      def self.patterns
        [
          {
            proofir: "pre(predicate)",
            ruby: lambda { |predicate| "# @pre #{predicate}" },
            inverse: lambda { |line| line.to_s[/#\s*@pre\s+(.+)/, 1] },
            role: "pre",
            cid: CID
          },
          {
            proofir: "post(predicate)",
            ruby: lambda { |predicate| "# @post #{predicate}" },
            inverse: lambda { |line| line.to_s[/#\s*@post\s+(.+)/, 1] },
            role: "post",
            cid: CID
          },
          {
            proofir: "invariant(predicate)",
            ruby: lambda { |predicate| "# @invariant #{predicate}" },
            inverse: lambda { |line| line.to_s[/#\s*@invariant\s+(.+)/, 1] },
            role: "invariant",
            cid: CID
          }
        ]
      end
    end
  end
end
