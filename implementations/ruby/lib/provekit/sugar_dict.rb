require "provekit/sugars/sorbet_sigs"
require "provekit/sugars/dry_validation"
require "provekit/sugars/contracts_ruby"
require "provekit/sugars/comment_roles"
require "provekit/ruby_ast_template"

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
      with_body_sources(
        Sugars::SorbetSigs.patterns +
        Sugars::DryValidation.patterns +
        Sugars::ContractsRuby.patterns +
        Sugars::CommentRoles.patterns
      )
    end

    def self.with_body_sources(patterns)
      patterns.map do |pattern|
        ruby = pattern[:ruby]
        next pattern unless ruby.respond_to?(:call)

        params = ruby.parameters.each_with_index.map do |param, index|
          name = param[1]
          name ? name.to_s : "arg#{index + 1}"
        end
        body_text = ruby.call(*params)
        pattern.merge(
          body_source: Provekit::RubyAstTemplate.body_source_for_body(
            body_text,
            params,
            file: "<sugar-dict>",
          )
        )
      end
    end
  end
end
