#include "cpp_source_lifter.hpp"

#include "provekit/canonicalizer/jcs.hpp"

#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>

namespace {

using provekit::cpp_source::CompileBodyOptions;
using provekit::cpp_source::LiftResult;
using provekit::cpp_source::canonical_bytes;
using provekit::cpp_source::compile_body_term;
using provekit::cpp_source::find_contract;
using provekit::cpp_source::lift_source;
using provekit::cpp_source::post_rhs;

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
    test_refuses_unsupported_lambda_without_unknown_ops();
    test_effects_use_canonical_shapes_and_sort_order();
    test_round_trip_body_term_is_byte_identical();
    test_initialize_reports_cpp_source_draft();
    std::cout << "cpp-source lifter tests passed\n";
    return 0;
}
