require "provekit/ruby_lifter"
require "provekit/sugar_dict"

module Provekit
  class RubyRealizer
    def realize(proofir, plan = {})
      declarations = proofir["declarations"] || proofir["ir"] || []
      requested = Array(plan["sugar_cids"] || plan["sugars"])
      requested = Provekit::SugarDict.default_cids if requested.empty?
      used = []
      source = declarations.map do |decl|
        used.concat(realization_sugars(decl, requested))
        render_contract(decl, requested)
      end.join("\n")
      source << "\n" unless source.end_with?("\n")

      lifter = RubyLifter.new
      {
        "emitted_source" => source,
        "emitted_artifact_cid" => lifter.cid_for({ "language" => "ruby", "source" => source }),
        "observed_loss_record" => [],
        "used_sugars" => used.uniq
      }
    end

    private

    def render_contract(decl, requested)
      lines = []
      sig = decl.dig("evidence", "sorbet_sig")
      comment_roles = decl.dig("evidence", "comments") || {}
      if sig && requested.include?("provekit:sugar:ruby:sorbet-sigs:v1")
        lines << sig["source"]
      end
      %w[pre post invariant].each do |role|
        Array(comment_roles[role]).each { |text| lines << "# @#{role} #{text}" }
      end

      params = Array(decl["params"])
      lines << "def #{decl["name"]}(#{params.join(", ")})"
      Array(decl.dig("evidence", "values")).each do |binding|
        lines << "  #{binding["name"]} = #{binding["term"]}"
      end
      Array(decl.dig("evidence", "effects")).each do |effect|
        next unless effect["kind"] == "Panics"
        lines << "  #{effect["surface"]}"
      end
      lines << render_return(decl)
      lines << "end"
      lines.join("\n")
    end

    def render_return(decl)
      ret = decl.dig("evidence", "sorbet_sig", "returns")
      case ret
      when "Boolean", "T::Boolean" then "  true"
      when "Integer" then "  0"
      when "String" then "  \"\""
      else
        "  nil"
      end
    end

    def realization_sugars(decl, requested)
      used = []
      used << "provekit:sugar:ruby:sorbet-sigs:v1" if decl.dig("evidence", "sorbet_sig") && requested.include?("provekit:sugar:ruby:sorbet-sigs:v1")
      roles = decl.dig("evidence", "comments") || {}
      used << "provekit:sugar:ruby:comment-roles:v1" if roles.values.any? { |v| Array(v).any? }
      Array(decl.dig("evidence", "effects")).each do |effect|
        used << "provekit:sugar:ruby:contracts-ruby:v1" if effect["kind"] == "Panics"
      end
      used
    end
  end
end
