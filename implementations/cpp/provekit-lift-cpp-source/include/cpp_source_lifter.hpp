#pragma once

#include "provekit/canonicalizer/value.hpp"

#include <string>
#include <vector>

namespace provekit::cpp_source {

constexpr const char* VERSION = "0.1.0-draft";
constexpr const char* DIALECT = "cpp-source";
constexpr const char* IR_VERSION = "v1.1.0";

struct Refusal {
    std::string kind;
    std::string function;
    int line = 0;
    std::string reason;
};

struct LiftResult {
    std::vector<canonicalizer::ValuePtr> declarations;
    std::vector<Refusal> refusals;
    std::vector<canonicalizer::ValuePtr> diagnostics;
    std::vector<canonicalizer::ValuePtr> opacity_report;
};

struct SourceSpan {
    int start_line = 0;
    int start_col = 0;
    int end_line = 0;
    int end_col = 0;
};

struct BindingTemplate {
    std::string concept_name;
    std::string library_tag;
    canonicalizer::ValuePtr family;
    canonicalizer::ValuePtr ast_template;
    std::string template_cid;
    std::vector<std::string> param_names;
    std::string contract_cid;
    std::string source_function_name;
};

struct ParamBinding {
    int index = 0;
    std::string source_text;
};

struct RecognizeTag {
    std::string file;
    SourceSpan span;
    std::string function_name;
    std::string concept_name;
    std::string library_tag;
    canonicalizer::ValuePtr family;
    std::string template_cid;
    std::string contract_cid;
    std::string match_tier;
    std::vector<ParamBinding> param_bindings;
};

struct RecognizeResult {
    std::vector<RecognizeTag> tags;
    std::vector<Refusal> refusals;
    std::vector<canonicalizer::ValuePtr> diagnostics;
};

struct CompileBodyOptions {
    std::string function_name = "f";
    std::vector<std::string> formals;
    std::string return_type = "int";
};

canonicalizer::ValuePtr initialize_result();
LiftResult lift_source(const std::string& path, const std::string& source);
LiftResult lift_paths(const std::string& workspace_root, const std::vector<std::string>& source_paths);
RecognizeResult recognize_source(const std::string& path, const std::string& source, const std::vector<BindingTemplate>& bindings);
RecognizeResult recognize_paths(const std::string& project_root, const std::vector<std::string>& source_paths);
std::string compile_ir_document(const std::vector<canonicalizer::ValuePtr>& ir);
std::string compile_body_term(const canonicalizer::ValuePtr& term, const CompileBodyOptions& options = {});
canonicalizer::ValuePtr post_rhs(const canonicalizer::ValuePtr& contract);
const canonicalizer::ValuePtr* find_contract(const LiftResult& result, const std::string& name_fragment);
std::string canonical_bytes(const canonicalizer::ValuePtr& value);
std::string cid_of_value(const canonicalizer::ValuePtr& value);
std::string lift_result_json(const LiftResult& result);
std::string recognize_result_json(const RecognizeResult& result);
int run_rpc();

}  // namespace provekit::cpp_source
