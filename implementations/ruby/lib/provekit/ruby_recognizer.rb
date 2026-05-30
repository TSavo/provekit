require "pathname"

require "provekit/ruby_ast_template"
require "provekit/sugar_dict"

module Provekit
  module RubyRecognizer
    module_function

    def recognize(params)
      if params["source"]
        return recognize_text(
          params["source"].to_s,
          params["path"] || "source.rb",
          Array(params["binding_templates"]),
        )
      end

      project_root = params["project_root"].to_s
      raise ArgumentError, "missing `project_root`" if project_root.empty?

      source_paths = Array(params["source_paths"])
      raise ArgumentError, "missing `source_paths` array" if source_paths.empty?

      root = Pathname.new(project_root).realpath
      templates = Array(params["binding_templates"])
      tags = []

      source_paths.each do |requested|
        full = resolve_inside_root(root, requested)
        next unless full && full.exist?

        ruby_files(full).sort_by(&:to_s).each do |file|
          rel = file.relative_path_from(root).to_s
          tags.concat(recognize_text(File.read(file, encoding: "UTF-8"), rel, templates)["tags"])
        end
      end

      { "tags" => tags }
    end

    def recognize_text(source, file = "source.rb", binding_templates = [])
      templates = Array(binding_templates)
      templates = default_binding_templates if templates.empty?
      bindings = binding_templates_by_cid(templates)
      tags = []

      RubyAstTemplate.extract_methods(source, file).each do |method_info|
        body = method_info[:body_source]
        template_cid = body["template_cid"]
        binding = bindings[template_cid]
        next unless binding

        tags << {
          "file" => file,
          "span" => body["span"],
          "function_name" => method_info[:name],
          "concept_name" => binding["concept_name"],
          "library_tag" => binding["library_tag"],
          "family" => binding["family"],
          "template_cid" => template_cid,
          "contract_cid" => binding.key?("contract_cid") ? binding["contract_cid"] : nil,
          "match_tier" => "exact",
          "param_bindings" => method_info[:params].each_with_index.map do |name, index|
            { "index" => index + 1, "source_text" => name }
          end,
        }
      end

      { "tags" => tags }
    end

    def default_binding_templates
      Provekit::SugarDict.patterns.each_with_object([]) do |pattern, out|
        body = pattern[:body_source]
        next unless body && body["ast_template"] && body["template_cid"]

        out << {
          "concept_name" => pattern[:proofir],
          "library_tag" => pattern[:cid],
          "family" => "provekit:sugar:ruby",
          "ast_template" => body["ast_template"],
          "template_cid" => body["template_cid"],
          "param_names" => body["param_names"],
          "contract_cid" => nil,
          "source_function_name" => pattern[:role],
        }
      end
    end

    def binding_templates_by_cid(binding_templates)
      Array(binding_templates).each_with_object({}) do |binding, out|
        next unless binding.is_a?(Hash)

        cid = binding["template_cid"]
        cid = RubyAstTemplate.cid_for(binding["ast_template"]) if (cid.nil? || cid == "") && binding["ast_template"]
        next unless cid.is_a?(String) && !cid.empty?

        out[cid] = binding
      end
    end

    def resolve_inside_root(root, requested)
      path = Pathname.new(requested.to_s)
      full = path.absolute? ? path : root + path
      resolved = full.realpath
      relative_to?(resolved, root) ? resolved : nil
    rescue SystemCallError
      nil
    end

    def ruby_files(path)
      return [path] if path.file? && path.extname == ".rb"
      return [] unless path.directory?

      path.find.select { |entry| entry.file? && entry.extname == ".rb" }
    end

    def relative_to?(path, root)
      path.ascend.any? { |ancestor| ancestor == root }
    end
  end
end
