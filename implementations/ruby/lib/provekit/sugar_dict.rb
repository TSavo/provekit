require "provekit/sugars/sorbet_sigs"
require "provekit/sugars/dry_validation"
require "provekit/sugars/contracts_ruby"
require "provekit/sugars/comment_roles"

module Provekit
  module SugarDict
    def self.default_cids
      [
        "provekit:sugar:ruby:sorbet-sigs:v1",
        "provekit:sugar:ruby:dry-validation:v1",
        "provekit:sugar:ruby:contracts-ruby:v1",
        "provekit:sugar:ruby:comment-roles:v1"
      ]
    end

    def self.patterns
      Sugars::SorbetSigs.patterns +
        Sugars::DryValidation.patterns +
        Sugars::ContractsRuby.patterns +
        Sugars::CommentRoles.patterns
    end
  end
end
