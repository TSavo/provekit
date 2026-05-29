#include "cpp_source_lifter.hpp"

#include "provekit/canonicalizer/jcs.hpp"

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

namespace {

using provekit::cpp_source::CompileBodyOptions;
using provekit::cpp_source::BindingTemplate;
using provekit::cpp_source::LiftResult;
using provekit::cpp_source::canonical_bytes;
using provekit::cpp_source::cid_of_value;
using provekit::cpp_source::compile_body_term;
using provekit::cpp_source::find_contract;
using provekit::cpp_source::lift_source;
using provekit::cpp_source::post_rhs;
using provekit::cpp_source::recognize_paths;
using provekit::cpp_source::recognize_source;

void require(bool ok, const std::string& message) {
    if (!ok) {
        std::cerr << "FAIL: " << message << "\n";
        std::exit(1);
    }
}

std::string encoded(const provekit::canonicalizer::ValuePtr& value) {
    return provekit::canonicalizer::encode_jcs(value);
}

bool contains(const provekit::canonicalizer::ValuePtr& value, const std::string& needle) {
    return encoded(value).find(needle) != std::string::npos;
}

provekit::canonicalizer::ValuePtr field(const provekit::canonicalizer::ValuePtr& value, const std::string& key) {
    if (!value || value->kind() != provekit::canonicalizer::ValueKind::Object) return nullptr;
    for (const auto& [name, child] : value->as_object()) {
        if (name == key) return child;
    }
    return nullptr;
}

std::string string_field(const provekit::canonicalizer::ValuePtr& value, const std::string& key) {
    auto child = field(value, key);
    if (!child || child->kind() != provekit::canonicalizer::ValueKind::String) return "";
    return child->as_string();
}

provekit::canonicalizer::ValuePtr source_unit_body(const LiftResult& result) {
    for (const auto& item : result.declarations) {
        if (!contains(item, "\"cpp:source-unit\"")) continue;
        auto source_unit = post_rhs(item);
        auto args = field(source_unit, "args");
        require(args && args->kind() == provekit::canonicalizer::ValueKind::Array,
                "source-unit should have argument array");
        require(args->as_array().size() == 2, "source-unit should carry bytes and operational term");
        return args->as_array()[1];
    }
    return nullptr;
}

BindingTemplate binding_from_contract(const provekit::canonicalizer::ValuePtr& contract,
                                      std::string concept,
                                      std::string library_tag,
                                      std::string contract_cid) {
    auto body_source = field(contract, "body_source");
    require(body_source != nullptr, "contract should carry body_source for recognizer binding");
    auto ast_template = field(body_source, "ast_template");
    if (!ast_template) ast_template = field(body_source, "tree");
    require(ast_template != nullptr, "body_source should carry ast_template or tree");
    BindingTemplate binding;
    binding.concept_name = std::move(concept);
    binding.library_tag = std::move(library_tag);
    binding.family = provekit::canonicalizer::Value::string("concept:family:cpp-test");
    binding.ast_template = ast_template;
    binding.template_cid = string_field(body_source, "template_cid");
    binding.param_names = {"x", "y"};
    binding.contract_cid = std::move(contract_cid);
    binding.source_function_name = string_field(contract, "fnName");
    return binding;
}

std::string json_string(const std::string& s) {
    std::string out = "\"";
    for (char ch : s) {
        switch (ch) {
            case '\\': out += "\\\\"; break;
            case '"': out += "\\\""; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default: out.push_back(ch); break;
        }
    }
    out.push_back('"');
    return out;
}

std::filesystem::path temp_dir(const std::string& label) {
    for (int i = 0; i < 100; ++i) {
        auto path = std::filesystem::temp_directory_path() /
                    (label + "-" + std::to_string(std::rand()) + "-" + std::to_string(i));
        if (std::filesystem::create_directory(path)) return path;
    }
    require(false, "should create temp dir");
    return {};
}

void test_lift_simple_add_emits_contract_and_source_unit() {
    const std::string source = "int f(int x, int y) { return x + y; }\n";
    LiftResult result = lift_source("add.cpp", source);

    require(result.refusals.empty(), "simple add should not refuse");
    require(result.declarations.size() == 2, "simple add should emit source unit plus one contract");

    const auto& source_unit = result.declarations[0];
    require(contains(source_unit, "\"cpp:source-unit\""), "source unit should carry cpp:source-unit");
    require(contains(source_unit, "\"encoding\":\"hex\""), "source unit should carry real source bytes");

    const auto contract = find_contract(result, "f");
    require(contract != nullptr, "function contract for f should be present");
    require(contains(*contract, "\"kind\":\"function-contract\""), "contract kind should be function-contract");
    require(contains(*contract, "\"cpp:add\""), "contract postcondition should contain cpp:add");
    require(contains(*contract, "\"bodyCid\":\"blake3-512:"), "contract should carry a body CID");
    require(!contains(*contract, "cpp:unknown"), "IR must not contain cpp:unknown");
    require(!contains(*contract, "cpp:binop"), "IR must not contain cpp:binop");
}

void test_lifted_contract_carries_body_source_text_and_ast_template() {
    const std::string source = "int f(int x, int y) { return x + y; }\n";
    LiftResult result = lift_source("add.cpp", source);

    require(result.refusals.empty(), "body-source fixture should not refuse");
    const auto contract = find_contract(result, "f");
    require(contract != nullptr, "function contract for f should be present");
    auto body_source = field(*contract, "body_source");
    require(body_source != nullptr, "function contract should carry body_source");

    const std::string body_text = string_field(body_source, "body_text");
    require(body_text.find("return") != std::string::npos, "body_source.body_text should contain function body text");
    require(body_text.find("x + y") != std::string::npos, "body_source.body_text should preserve source expression");

    auto ast_template = field(body_source, "ast_template");
    if (!ast_template) ast_template = field(body_source, "tree");
    require(ast_template != nullptr, "body_source should carry ast_template or equivalent tree");
    require(contains(ast_template, "\"param_ref\""), "ast_template should normalize formal parameter references");
    require(string_field(body_source, "template_cid") == cid_of_value(ast_template),
            "body_source.template_cid should be the CID of ast_template");
}

void test_recognize_emits_exact_tag_for_alpha_equivalent_cpp_body() {
    LiftResult sugar = lift_source("shim.cpp", "int sugar(int x, int y) { return x + y; }\n");
    require(sugar.refusals.empty(), "sugar fixture should not refuse");
    const auto sugar_contract = find_contract(sugar, "sugar");
    require(sugar_contract != nullptr, "sugar contract missing");

    BindingTemplate binding = binding_from_contract(*sugar_contract,
                                                    "concept:cpp-add",
                                                    "provekit-shim-cpp-test",
                                                    "blake3-512:contract");
    auto recognized = recognize_source("user.cpp",
                                       "int user(int left, int right) { return left + right; }\n",
                                       {binding});

    require(recognized.tags.size() == 1, "alpha-equivalent body should emit one exact tag");
    const auto& tag = recognized.tags[0];
    require(tag.file == "user.cpp", "recognize tag should carry source file");
    require(tag.function_name.find("user") != std::string::npos, "recognize tag should carry function name");
    require(tag.concept_name == "concept:cpp-add", "recognize tag should carry concept");
    require(tag.library_tag == "provekit-shim-cpp-test", "recognize tag should carry library tag");
    require(tag.template_cid == binding.template_cid, "recognize tag should carry matched template CID");
    require(tag.contract_cid == "blake3-512:contract", "recognize tag should carry contract CID");
    require(tag.match_tier == "exact", "recognize tag should be exact");
    require(tag.param_bindings.size() == 2, "recognize tag should carry parameter bindings");
    require(tag.param_bindings[0].source_text == "left", "first parameter binding should use user source name");
    require(tag.param_bindings[1].source_text == "right", "second parameter binding should use user source name");
    require(tag.span.start_line == 1 && tag.span.end_line == 1, "recognize tag should carry useful source span");
}

void test_recognize_does_not_match_different_cpp_body() {
    LiftResult sugar = lift_source("shim.cpp", "int sugar(int x, int y) { return x + y; }\n");
    require(sugar.refusals.empty(), "sugar fixture should not refuse");
    const auto sugar_contract = find_contract(sugar, "sugar");
    require(sugar_contract != nullptr, "sugar contract missing");

    BindingTemplate binding = binding_from_contract(*sugar_contract,
                                                    "concept:cpp-add",
                                                    "provekit-shim-cpp-test",
                                                    "blake3-512:contract");
    auto recognized = recognize_source("user.cpp",
                                       "int user(int left, int right) { return left - right; }\n",
                                       {binding});

    require(recognized.tags.empty(), "different body should not emit an exact recognize tag");
}

void test_recognize_paths_resolves_templates_from_cpp_owned_proof_context() {
    LiftResult sugar = lift_source("shim.cpp", "int sugar(int x, int y) { return x + y; }\n");
    require(sugar.refusals.empty(), "sugar fixture should not refuse");
    const auto sugar_contract = find_contract(sugar, "sugar");
    require(sugar_contract != nullptr, "sugar contract missing");
    BindingTemplate binding = binding_from_contract(*sugar_contract,
                                                    "concept:cpp-add",
                                                    "provekit-shim-cpp-test",
                                                    "blake3-512:contract");

    auto root = temp_dir("provekit-cpp-recognize");
    std::filesystem::create_directories(root / "src");
    {
        std::ofstream out(root / "src" / "user.cpp");
        out << "int user(int left, int right) { return left + right; }\n";
    }
    {
        std::ofstream proof(root / "cpp-binding.proof");
        proof << "{\"members\":[{\"kind\":\"library-sugar-binding-entry\","
              << "\"target_language\":\"cpp\","
              << "\"concept_name\":\"concept:cpp-add\","
              << "\"target_library_tag\":\"provekit-shim-cpp-test\","
              << "\"family\":\"concept:family:cpp-test\","
              << "\"source_function_name\":\"sugar\","
              << "\"contract_cid\":\"blake3-512:contract\","
              << "\"body_source\":{"
              << "\"body_text\":" << json_string(string_field(field(*sugar_contract, "body_source"), "body_text")) << ","
              << "\"ast_template\":" << encoded(binding.ast_template) << ","
              << "\"template_cid\":" << json_string(binding.template_cid) << ","
              << "\"param_names\":[\"x\",\"y\"]"
              << "}}]}";
    }

    auto recognized = recognize_paths(root.string(), {"src/user.cpp"});
    std::filesystem::remove_all(root);

    require(recognized.tags.size() == 1,
            "CLI-facing recognize path should resolve templates inside the C++ kit without binding_templates");
    require(recognized.tags[0].template_cid == binding.template_cid,
            "kit-owned proof resolution should feed recognizer template CID");
}

void test_refuses_unsupported_lambda_without_unknown_ops() {
    const std::string source =
        "int f(int x) {\n"
        "  auto g = [x]() { return x + 1; };\n"
        "  return g();\n"
        "}\n";
    LiftResult result = lift_source("lambda.cpp", source);

    require(result.declarations.empty(), "lambda body should not emit a fake contract");
    require(result.refusals.size() == 1, "lambda body should emit one refusal");
    require(result.refusals[0].kind == "LambdaExpr", "refusal should name the unsupported AST kind");
    require(result.refusals[0].function.find("f") != std::string::npos, "refusal should carry function identity");
    require(result.refusals[0].line == 2, "refusal should carry source line");
    require(result.refusals[0].reason.find("lambda") != std::string::npos, "refusal reason should be explicit");
}

void test_effects_use_canonical_shapes_and_sort_order() {
    const std::string source =
        "extern \"C\" int printf(const char*, ...);\n"
        "int G;\n"
        "int missing(int);\n"
        "int f(int *p, int x) {\n"
        "  G = *p;\n"
        "  printf(\"%d\", G);\n"
        "  while (x > 0) { x = x - 1; }\n"
        "  missing(G);\n"
        "  if (x < 0) { throw x; }\n"
        "  return G;\n"
        "}\n";
    LiftResult result = lift_source("effects.cpp", source);

    require(result.refusals.empty(), "effects fixture should be modeled without refusals");
    const auto contract = find_contract(result, "f");
    require(contract != nullptr, "effects fixture should emit f contract");
    const std::string json = encoded(*contract);
    const std::string want =
        "\"effects\":["
        "{\"kind\":\"reads\",\"target\":\"G\"},"
        "{\"kind\":\"writes\",\"target\":\"G\"},"
        "{\"kind\":\"io\"},"
        "{\"kind\":\"panics\"},"
        "{\"kind\":\"unresolved_call\",\"name\":\"missing\"},"
        "{\"kind\":\"opaque_loop\",\"loopCid\":\"blake3-512:";
    require(json.find(want) != std::string::npos, "effects should use canonical shapes and Rust sort order");
}

void test_round_trip_body_term_is_byte_identical() {
    const std::string source = "int f(int x, int y) { return x + y; }\n";
    LiftResult first = lift_source("roundtrip.cpp", source);
    require(first.refusals.empty(), "first round-trip lift should not refuse");
    const auto first_contract = find_contract(first, "f");
    require(first_contract != nullptr, "first round-trip contract missing");
    auto body = post_rhs(*first_contract);

    CompileBodyOptions options;
    options.function_name = "f";
    options.formals = {"x", "y"};
    options.return_type = "int";
    const std::string compiled = compile_body_term(body, options);

    LiftResult second = lift_source("roundtrip.cpp", compiled);
    require(second.refusals.empty(), "compiled body should relift without refusals");
    const auto second_contract = find_contract(second, "f");
    require(second_contract != nullptr, "second round-trip contract missing");
    auto relifted_body = post_rhs(*second_contract);

    require(canonical_bytes(body) == canonical_bytes(relifted_body),
            "bare body compile-lift round trip should be byte-identical");
}

void test_for_loop_postinc_round_trip_body_term_is_byte_identical() {
    const std::string source =
        "int sum_to(int n) {\n"
        "  int s = 0;\n"
        "  for (int i = 0; i < n; i++) { s = s + i; }\n"
        "  return s;\n"
        "}\n";
    LiftResult first = lift_source("for_postinc.cpp", source);
    require(first.refusals.empty(), "for-loop postinc lift should not refuse");
    const auto first_contract = find_contract(first, "sum_to");
    require(first_contract != nullptr, "for-loop postinc contract missing");
    auto body = source_unit_body(first);
    require(body != nullptr, "for-loop source-unit operational body missing");
    require(contains(body, "\"cpp:postinc\""), "for-loop update should lift to cpp:postinc");

    CompileBodyOptions options;
    options.function_name = "sum_to";
    options.formals = {"n"};
    options.return_type = "int";
    const std::string compiled = compile_body_term(body, options);

    LiftResult second = lift_source("for_postinc.cpp", compiled);
    require(second.refusals.empty(), "compiled for-loop body should relift without refusals");
    const auto second_contract = find_contract(second, "sum_to");
    require(second_contract != nullptr, "relifted for-loop postinc contract missing");
    auto relifted_body = source_unit_body(second);
    require(relifted_body != nullptr, "relifted for-loop source-unit operational body missing");

    require(canonical_bytes(body) == canonical_bytes(relifted_body),
            "for-loop postinc body compile-lift round trip should be byte-identical");
}

void test_initialize_reports_cpp_source_draft() {
    const auto init = provekit::cpp_source::initialize_result();
    const std::string json = encoded(init);
    require(json.find("\"version\":\"0.1.0-draft\"") != std::string::npos,
            "initialize should report 0.1.0-draft");
    require(json.find("\"authoring_surfaces\":[\"cpp-source\"]") != std::string::npos,
            "initialize should report cpp-source surface");
    require(json.find("\"emits_signed_mementos\":false") != std::string::npos,
            "initialize must not claim signed mementos");
}

}  // namespace

int main() {
    test_lift_simple_add_emits_contract_and_source_unit();
    test_lifted_contract_carries_body_source_text_and_ast_template();
    test_recognize_emits_exact_tag_for_alpha_equivalent_cpp_body();
    test_recognize_does_not_match_different_cpp_body();
    test_recognize_paths_resolves_templates_from_cpp_owned_proof_context();
    test_refuses_unsupported_lambda_without_unknown_ops();
    test_effects_use_canonical_shapes_and_sort_order();
    test_round_trip_body_term_is_byte_identical();
    test_for_loop_postinc_round_trip_body_term_is_byte_identical();
    test_initialize_reports_cpp_source_draft();
    std::cout << "cpp-source lifter tests passed\n";
    return 0;
}
