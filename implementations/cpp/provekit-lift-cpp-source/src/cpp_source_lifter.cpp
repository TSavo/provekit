#include "cpp_source_lifter.hpp"

#include "provekit/canonicalizer/hash.hpp"
#include "provekit/canonicalizer/jcs.hpp"

#include <clang-c/Index.h>

#include <algorithm>
#include <cctype>
#include <cerrno>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <map>
#include <optional>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace provekit::cpp_source {
namespace {

using canonicalizer::Value;
using canonicalizer::ValueKind;
using canonicalizer::ValuePtr;

ValuePtr nullv() { return Value::null_value(); }
ValuePtr boolv(bool v) { return Value::boolean(v); }
ValuePtr intv(int64_t v) { return Value::integer(v); }
ValuePtr strv(std::string v) { return Value::string(std::move(v)); }
ValuePtr arr(std::vector<ValuePtr> values) { return Value::array(std::move(values)); }
ValuePtr obj(std::vector<std::pair<std::string, ValuePtr>> values) { return Value::object(std::move(values)); }

ValuePtr prim_sort(const std::string& name) {
    return obj({{"kind", strv("primitive")}, {"name", strv(name)}});
}

ValuePtr var_term(const std::string& name) {
    return obj({{"kind", strv("var")}, {"name", strv(name)}});
}

ValuePtr const_term(ValuePtr value, const std::string& sort) {
    return obj({{"kind", strv("const")}, {"sort", prim_sort(sort)}, {"value", std::move(value)}});
}

ValuePtr int_const(int64_t value) { return const_term(intv(value), "Int"); }
ValuePtr bool_const(bool value) { return const_term(boolv(value), "Bool"); }
ValuePtr string_const(const std::string& value) { return const_term(strv(value), "String"); }
ValuePtr unit_const() { return const_term(intv(0), "Unit"); }

ValuePtr bytes_term(const std::string& bytes) {
    static constexpr char hex[] = "0123456789abcdef";
    std::string encoded;
    encoded.reserve(bytes.size() * 2);
    for (unsigned char c : bytes) {
        encoded.push_back(hex[(c >> 4) & 0x0f]);
        encoded.push_back(hex[c & 0x0f]);
    }
    return obj({{"encoding", strv("hex")}, {"kind", strv("bytes")}, {"value", strv(encoded)}});
}

ValuePtr ctor(const std::string& name, std::vector<ValuePtr> args = {}) {
    return obj({{"args", arr(std::move(args))}, {"kind", strv("ctor")}, {"name", strv(name)}});
}

ValuePtr true_formula() {
    return obj({{"args", arr({})}, {"kind", strv("atomic")}, {"name", strv("true")}});
}

ValuePtr eq_formula(ValuePtr lhs, ValuePtr rhs) {
    return obj({{"args", arr({std::move(lhs), std::move(rhs)})}, {"kind", strv("atomic")}, {"name", strv("=")}});
}

ValuePtr locus_value(const std::string& file, int line, int col) {
    return obj({{"col", intv(col)}, {"file", strv(file)}, {"line", intv(line)}});
}

ValuePtr span_value(const SourceSpan& span) {
    return obj({
        {"end_col", intv(span.end_col)},
        {"end_line", intv(span.end_line)},
        {"start_col", intv(span.start_col)},
        {"start_line", intv(span.start_line)},
    });
}

ValuePtr empty_array() { return arr({}); }

ValuePtr get_field(const ValuePtr& value, const std::string& key) {
    if (!value || value->kind() != ValueKind::Object) return nullptr;
    for (const auto& [k, v] : value->as_object()) {
        if (k == key) return v;
    }
    return nullptr;
}

std::string get_string(const ValuePtr& value, const std::string& fallback = "") {
    if (!value || value->kind() != ValueKind::String) return fallback;
    return value->as_string();
}

bool is_kind_name(const ValuePtr& value, const std::string& kind, const std::string& name) {
    return get_string(get_field(value, "kind")) == kind && get_string(get_field(value, "name")) == name;
}

std::vector<ValuePtr> term_args(const ValuePtr& term) {
    auto args = get_field(term, "args");
    if (!args || args->kind() != ValueKind::Array) return {};
    return args->as_array();
}

std::string cx_string(CXString value) {
    const char* raw = clang_getCString(value);
    std::string out = raw ? raw : "";
    clang_disposeString(value);
    return out;
}

std::string cursor_spelling(CXCursor cursor) { return cx_string(clang_getCursorSpelling(cursor)); }
std::string cursor_usr(CXCursor cursor) { return cx_string(clang_getCursorUSR(cursor)); }
std::string cursor_kind_name(CXCursor cursor) { return cx_string(clang_getCursorKindSpelling(clang_getCursorKind(cursor))); }
std::string type_spelling(CXType type) { return cx_string(clang_getTypeSpelling(type)); }

struct SourceLoc {
    int line = 0;
    int col = 0;
};

SourceLoc cursor_loc(CXCursor cursor) {
    CXSourceLocation location = clang_getCursorLocation(cursor);
    unsigned line = 0;
    unsigned column = 0;
    clang_getExpansionLocation(location, nullptr, &line, &column, nullptr);
    return {static_cast<int>(line), static_cast<int>(column)};
}

SourceSpan cursor_span(CXCursor cursor) {
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXSourceLocation start = clang_getRangeStart(range);
    CXSourceLocation end = clang_getRangeEnd(range);
    unsigned start_line = 0;
    unsigned start_column = 0;
    unsigned end_line = 0;
    unsigned end_column = 0;
    clang_getExpansionLocation(start, nullptr, &start_line, &start_column, nullptr);
    clang_getExpansionLocation(end, nullptr, &end_line, &end_column, nullptr);
    return {
        static_cast<int>(start_line),
        start_column > 0 ? static_cast<int>(start_column - 1) : 0,
        static_cast<int>(end_line),
        static_cast<int>(end_column),
    };
}

std::optional<std::pair<unsigned, unsigned>> cursor_offsets(CXCursor cursor) {
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXSourceLocation start = clang_getRangeStart(range);
    CXSourceLocation end = clang_getRangeEnd(range);
    unsigned start_offset = 0;
    unsigned end_offset = 0;
    clang_getExpansionLocation(start, nullptr, nullptr, nullptr, &start_offset);
    clang_getExpansionLocation(end, nullptr, nullptr, nullptr, &end_offset);
    if (end_offset < start_offset) return std::nullopt;
    return std::make_pair(start_offset, end_offset);
}

bool from_main_file(CXCursor cursor) {
    return clang_Location_isFromMainFile(clang_getCursorLocation(cursor)) != 0;
}

std::vector<CXCursor> children(CXCursor cursor) {
    std::vector<CXCursor> out;
    clang_visitChildren(
        cursor,
        [](CXCursor child, CXCursor, CXClientData data) {
            auto* vec = static_cast<std::vector<CXCursor>*>(data);
            vec->push_back(child);
            return CXChildVisit_Continue;
        },
        &out);
    return out;
}

std::vector<std::string> cursor_tokens(CXCursor cursor) {
    std::vector<std::string> out;
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken* tokens = nullptr;
    unsigned count = 0;
    clang_tokenize(tu, range, &tokens, &count);
    for (unsigned i = 0; i < count; ++i) {
        out.push_back(cx_string(clang_getTokenSpelling(tu, tokens[i])));
    }
    clang_disposeTokens(tu, tokens, count);
    return out;
}

std::string cursor_source(CXCursor cursor) {
    auto toks = cursor_tokens(cursor);
    std::ostringstream out;
    bool first = true;
    for (const auto& tok : toks) {
        if (!first) out << ' ';
        out << tok;
        first = false;
    }
    return out.str();
}

std::string normalize_source_text(std::string text) {
    std::string out;
    bool pending_space = false;
    for (char ch : text) {
        if (std::isspace(static_cast<unsigned char>(ch))) {
            pending_space = true;
        } else {
            if (pending_space && !out.empty()) out.push_back(' ');
            out.push_back(ch);
            pending_space = false;
        }
    }
    return out;
}

std::string trim_ws(const std::string& text) {
    size_t start = 0;
    while (start < text.size() && std::isspace(static_cast<unsigned char>(text[start]))) ++start;
    size_t end = text.size();
    while (end > start && std::isspace(static_cast<unsigned char>(text[end - 1]))) --end;
    return text.substr(start, end - start);
}

std::string qualified_name(CXCursor cursor) {
    std::vector<std::string> parts;
    CXCursor current = cursor;
    while (!clang_Cursor_isNull(current)) {
        CXCursorKind kind = clang_getCursorKind(current);
        if (kind == CXCursor_TranslationUnit) break;
        std::string name = cursor_spelling(current);
        if (!name.empty() && kind != CXCursor_LinkageSpec) parts.push_back(name);
        current = clang_getCursorSemanticParent(current);
    }
    std::reverse(parts.begin(), parts.end());
    std::ostringstream out;
    for (size_t i = 0; i < parts.size(); ++i) {
        if (i > 0) out << "::";
        out << parts[i];
    }
    return out.str();
}

std::string stable_function_name(CXCursor cursor) {
    std::string usr = cursor_usr(cursor);
    if (!usr.empty()) return usr;
    std::string q = qualified_name(cursor);
    return q.empty() ? cursor_spelling(cursor) : q;
}

bool type_is_scalar_or_ref(CXType type) {
    CXType canonical = clang_getCanonicalType(type);
    switch (canonical.kind) {
        case CXType_Void:
        case CXType_Bool:
        case CXType_Char_U:
        case CXType_UChar:
        case CXType_Char16:
        case CXType_Char32:
        case CXType_UShort:
        case CXType_UInt:
        case CXType_ULong:
        case CXType_ULongLong:
        case CXType_Char_S:
        case CXType_SChar:
        case CXType_WChar:
        case CXType_Short:
        case CXType_Int:
        case CXType_Long:
        case CXType_LongLong:
        case CXType_Float:
        case CXType_Double:
        case CXType_LongDouble:
        case CXType_Pointer:
        case CXType_LValueReference:
        case CXType_RValueReference:
            return true;
        default:
            return false;
    }
}

std::string sort_name_for_type(CXType type) {
    CXType canonical = clang_getCanonicalType(type);
    switch (canonical.kind) {
        case CXType_Void:
            return "Unit";
        case CXType_Bool:
            return "Bool";
        case CXType_Float:
        case CXType_Double:
        case CXType_LongDouble:
            return "Real";
        case CXType_Pointer:
        case CXType_LValueReference:
        case CXType_RValueReference:
            return "Ref";
        default:
            return "Int";
    }
}

bool is_function_cursor(CXCursorKind kind) {
    return kind == CXCursor_FunctionDecl || kind == CXCursor_CXXMethod;
}

bool is_unsupported_definition_cursor(CXCursorKind kind) {
    return kind == CXCursor_Constructor || kind == CXCursor_Destructor || kind == CXCursor_ConversionFunction;
}

std::string parse_top_level_operator(const std::vector<std::string>& tokens) {
    int paren = 0;
    int bracket = 0;
    int brace = 0;
    static const std::vector<std::string> ops = {
        "<<=", ">>=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "=",
        "||", "&&", "==", "!=", "<=", ">=", "<<", ">>", "+", "-", "*", "/", "%", "<", ">", "&", "|", "^"};
    for (const auto& tok : tokens) {
        if (tok == "(" || tok == "<") ++paren;
        else if (tok == ")" || tok == ">") --paren;
        else if (tok == "[") ++bracket;
        else if (tok == "]") --bracket;
        else if (tok == "{") ++brace;
        else if (tok == "}") --brace;
        if (paren == 0 && bracket == 0 && brace == 0) {
            for (const auto& op : ops) {
                if (tok == op) return tok;
            }
        }
    }
    for (const auto& tok : tokens) {
        for (const auto& op : ops) {
            if (tok == op) return tok;
        }
    }
    return "";
}

std::string binary_op_name(const std::string& op) {
    if (op == "+") return "cpp:add";
    if (op == "-") return "cpp:sub";
    if (op == "*") return "cpp:mul";
    if (op == "/") return "cpp:div";
    if (op == "%") return "cpp:mod";
    if (op == "==") return "cpp:eq";
    if (op == "!=") return "cpp:ne";
    if (op == "<") return "cpp:lt";
    if (op == "<=") return "cpp:le";
    if (op == ">") return "cpp:gt";
    if (op == ">=") return "cpp:ge";
    if (op == "&&") return "cpp:and";
    if (op == "||") return "cpp:or";
    if (op == "&") return "cpp:bitand";
    if (op == "|") return "cpp:bitor";
    if (op == "^") return "cpp:bitxor";
    if (op == "<<") return "cpp:shl";
    if (op == ">>") return "cpp:shr";
    return "";
}

std::string compound_op_name(const std::string& op) {
    if (op == "+=") return "cpp:add";
    if (op == "-=") return "cpp:sub";
    if (op == "*=") return "cpp:mul";
    if (op == "/=") return "cpp:div";
    if (op == "%=") return "cpp:mod";
    if (op == "&=") return "cpp:bitand";
    if (op == "|=") return "cpp:bitor";
    if (op == "^=") return "cpp:bitxor";
    if (op == "<<=") return "cpp:shl";
    if (op == ">>=") return "cpp:shr";
    return "";
}

std::string unary_prefix_op(const std::vector<std::string>& tokens) {
    for (const auto& tok : tokens) {
        if (tok == "++" || tok == "--" || tok == "!" || tok == "~" || tok == "-" || tok == "+" || tok == "*" || tok == "&") return tok;
        if (!tok.empty() && (std::isalnum(static_cast<unsigned char>(tok[0])) || tok[0] == '_')) return "";
    }
    return "";
}

bool is_postfix_incdec(const std::vector<std::string>& tokens) {
    return !tokens.empty() && (tokens.back() == "++" || tokens.back() == "--");
}

std::string literal_token(CXCursor cursor) {
    auto toks = cursor_tokens(cursor);
    return toks.empty() ? "0" : toks.front();
}

int64_t parse_integer_literal(std::string token) {
    token.erase(std::remove(token.begin(), token.end(), '\''), token.end());
    while (!token.empty() && std::isalpha(static_cast<unsigned char>(token.back()))) token.pop_back();
    int base = 10;
    if (token.size() > 2 && token[0] == '0' && (token[1] == 'x' || token[1] == 'X')) base = 16;
    else if (token.size() > 1 && token[0] == '0') base = 8;
    char* end = nullptr;
    errno = 0;
    long long value = std::strtoll(token.c_str(), &end, base);
    if (errno != 0 || end == token.c_str()) return 0;
    return static_cast<int64_t>(value);
}

std::string unquote_string_literal(const std::string& token) {
    size_t first = token.find('"');
    size_t last = token.rfind('"');
    if (first == std::string::npos || last == std::string::npos || first == last) return token;
    std::string body = token.substr(first + 1, last - first - 1);
    std::string out;
    for (size_t i = 0; i < body.size(); ++i) {
        if (body[i] == '\\' && i + 1 < body.size()) {
            char next = body[++i];
            switch (next) {
                case 'n': out.push_back('\n'); break;
                case 'r': out.push_back('\r'); break;
                case 't': out.push_back('\t'); break;
                case '\\': out.push_back('\\'); break;
                case '"': out.push_back('"'); break;
                default: out.push_back(next); break;
            }
        } else {
            out.push_back(body[i]);
        }
    }
    return out;
}

struct Unsupported : std::runtime_error {
    std::string kind;
    int line;
    Unsupported(std::string k, int l, std::string reason)
        : std::runtime_error(std::move(reason)), kind(std::move(k)), line(l) {}
};

Unsupported unsupported(CXCursor cursor, const std::string& reason) {
    return Unsupported(cursor_kind_name(cursor), cursor_loc(cursor).line, reason);
}

struct Effect {
    std::string kind;
    std::string target;
    std::string name;
    std::string loop_cid;
};

ValuePtr effect_value(const Effect& effect) {
    if (effect.kind == "reads" || effect.kind == "writes") {
        return obj({{"kind", strv(effect.kind)}, {"target", strv(effect.target)}});
    }
    if (effect.kind == "unresolved_call") {
        return obj({{"kind", strv("unresolved_call")}, {"name", strv(effect.name)}});
    }
    if (effect.kind == "opaque_loop") {
        return obj({{"kind", strv("opaque_loop")}, {"loopCid", strv(effect.loop_cid)}});
    }
    return obj({{"kind", strv(effect.kind)}});
}

std::string effect_sort_key(const Effect& effect) {
    if (effect.kind == "reads") return "0:reads:" + effect.target;
    if (effect.kind == "writes") return "1:writes:" + effect.target;
    if (effect.kind == "io") return "2:io";
    if (effect.kind == "unsafe") return "3:unsafe";
    if (effect.kind == "panics") return "4:panics";
    if (effect.kind == "unresolved_call") return "5:unresolved:" + effect.name;
    if (effect.kind == "opaque_loop") return "6:opaque_loop:" + effect.loop_cid;
    return "9:" + effect.kind;
}

struct EffectSet {
    std::map<std::string, Effect> effects;

    void add(Effect effect) {
        if (effect.kind.empty()) return;
        effects[effect_sort_key(effect)] = std::move(effect);
    }

    std::vector<ValuePtr> values() const {
        std::vector<ValuePtr> out;
        for (const auto& [_, effect] : effects) out.push_back(effect_value(effect));
        return out;
    }
};

struct StmtResult {
    ValuePtr term;
    ValuePtr return_term;
    bool has_return = false;
};

struct ExprResult {
    ValuePtr term;
};

ValuePtr seq_term(const std::vector<ValuePtr>& terms) {
    if (terms.empty()) return ctor("cpp:skip", {unit_const()});
    if (terms.size() == 1) return terms.front();
    return ctor("cpp:seq", terms);
}

bool cursor_is_global_var(CXCursor cursor) {
    if (clang_getCursorKind(cursor) != CXCursor_VarDecl) return false;
    CXCursor parent = clang_getCursorSemanticParent(cursor);
    CXCursorKind parent_kind = clang_getCursorKind(parent);
    return parent_kind == CXCursor_TranslationUnit || parent_kind == CXCursor_Namespace;
}

bool name_mentions_io(const std::string& name) {
    return name == "printf" || name == "puts" || name == "fprintf" || name == "fputs" ||
           name == "fopen" || name == "fclose" || name == "fread" || name == "fwrite" ||
           name.find("std::cout") != std::string::npos || name.find("std::cerr") != std::string::npos ||
           name.find("basic_ostream") != std::string::npos || name.find("operator<<") != std::string::npos;
}

bool is_pure_builtin(const std::string& name) {
    return name == "abs" || name == "std::abs" || name == "min" || name == "max" || name == "std::min" || name == "std::max";
}

struct LiftContext {
    std::string path;
    std::set<std::string> known_function_usrs;
    std::set<std::string> global_var_usrs;
    std::map<std::string, std::string> global_cells;
    std::set<std::string> locals;
    std::set<std::string> local_usrs;
    EffectSet effects;
    std::string function_name;

    bool is_local_ref(CXCursor referenced) const {
        std::string usr = cursor_usr(referenced);
        if (!usr.empty() && local_usrs.count(usr)) return true;
        return locals.count(cursor_spelling(referenced)) != 0;
    }

    bool is_global_ref(CXCursor referenced) const {
        std::string usr = cursor_usr(referenced);
        return !usr.empty() && global_var_usrs.count(usr) != 0;
    }

    std::string global_cell(CXCursor referenced) const {
        std::string usr = cursor_usr(referenced);
        auto it = global_cells.find(usr);
        if (it != global_cells.end()) return it->second;
        std::string q = qualified_name(referenced);
        return q.empty() ? cursor_spelling(referenced) : q;
    }
};

ValuePtr lift_expr(CXCursor cursor, LiftContext& ctx);
ValuePtr lift_target(CXCursor cursor, LiftContext& ctx);
StmtResult lift_stmt(CXCursor cursor, LiftContext& ctx);

void add_write_effect_for_target(CXCursor cursor, LiftContext& ctx) {
    CXCursorKind kind = clang_getCursorKind(cursor);
    if (kind == CXCursor_DeclRefExpr) {
        CXCursor referenced = clang_getCursorReferenced(cursor);
        if (ctx.is_global_ref(referenced)) ctx.effects.add({"writes", ctx.global_cell(referenced), "", ""});
        return;
    }
    if (kind == CXCursor_UnexposedExpr || kind == CXCursor_ParenExpr) {
        auto kids = children(cursor);
        if (kids.size() == 1) add_write_effect_for_target(kids.front(), ctx);
        return;
    }
    if (kind == CXCursor_UnaryOperator) {
        auto toks = cursor_tokens(cursor);
        if (unary_prefix_op(toks) == "*") {
            auto kids = children(cursor);
            std::string target = kids.empty() ? cursor_source(cursor) : "*" + normalize_source_text(cursor_source(kids.front()));
            ctx.effects.add({"writes", target, "", ""});
        }
        return;
    }
    if (kind == CXCursor_MemberRefExpr || kind == CXCursor_ArraySubscriptExpr) {
        auto kids = children(cursor);
        if (!kids.empty()) {
            CXCursor base = kids.front();
            if (clang_getCursorKind(base) == CXCursor_DeclRefExpr) {
                CXCursor ref = clang_getCursorReferenced(base);
                if (!ctx.is_local_ref(ref)) ctx.effects.add({"writes", normalize_source_text(cursor_source(cursor)), "", ""});
            } else {
                ctx.effects.add({"writes", normalize_source_text(cursor_source(cursor)), "", ""});
            }
        }
    }
}

ValuePtr lift_decl_ref(CXCursor cursor, LiftContext& ctx) {
    CXCursor referenced = clang_getCursorReferenced(cursor);
    if (ctx.is_global_ref(referenced)) ctx.effects.add({"reads", ctx.global_cell(referenced), "", ""});
    return var_term(cursor_spelling(cursor));
}

ValuePtr lift_call(CXCursor cursor, LiftContext& ctx) {
    CXCursor referenced = clang_getCursorReferenced(cursor);
    std::string callee = qualified_name(referenced);
    if (callee.empty()) callee = cursor_spelling(referenced);
    if (callee.empty()) {
        auto kids = children(cursor);
        if (!kids.empty()) callee = normalize_source_text(cursor_source(kids.front()));
    }

    std::vector<ValuePtr> args;
    args.push_back(string_const(callee));
    int argc = clang_Cursor_getNumArguments(cursor);
    for (int i = 0; i < argc; ++i) {
        args.push_back(lift_expr(clang_Cursor_getArgument(cursor, static_cast<unsigned>(i)), ctx));
    }

    std::string ref_usr = cursor_usr(referenced);
    if (name_mentions_io(callee)) {
        ctx.effects.add({"io", "", "", ""});
    } else if (!callee.empty() && !ctx.known_function_usrs.count(ref_usr) && !is_pure_builtin(callee)) {
        ctx.effects.add({"unresolved_call", "", callee, ""});
    }
    return ctor("cpp:call", std::move(args));
}

ValuePtr lift_binary(CXCursor cursor, LiftContext& ctx) {
    auto kids = children(cursor);
    if (kids.size() != 2) throw unsupported(cursor, "binary operator with non-binary AST shape is not modeled");
    std::string op = parse_top_level_operator(cursor_tokens(cursor));
    if (op == "=") {
        ValuePtr target = lift_target(kids[0], ctx);
        ValuePtr value = lift_expr(kids[1], ctx);
        add_write_effect_for_target(kids[0], ctx);
        return ctor("cpp:assign", {target, value});
    }
    std::string compound = compound_op_name(op);
    if (!compound.empty()) {
        ValuePtr target = lift_target(kids[0], ctx);
        ValuePtr value = lift_expr(kids[1], ctx);
        add_write_effect_for_target(kids[0], ctx);
        return ctor("cpp:assign", {target, ctor(compound, {target, value})});
    }
    std::string op_name = binary_op_name(op);
    if (op_name.empty()) throw unsupported(cursor, "binary operator '" + op + "' is not modeled");
    if (op == "<<" && normalize_source_text(cursor_source(kids[0])).find("std :: cout") != std::string::npos) {
        ctx.effects.add({"io", "", "", ""});
    }
    return ctor(op_name, {lift_expr(kids[0], ctx), lift_expr(kids[1], ctx)});
}

ValuePtr lift_unary(CXCursor cursor, LiftContext& ctx) {
    auto kids = children(cursor);
    if (kids.size() != 1) throw unsupported(cursor, "unary operator with unexpected AST shape is not modeled");
    auto toks = cursor_tokens(cursor);
    std::string op = is_postfix_incdec(toks) ? toks.back() : unary_prefix_op(toks);
    if (op == "-") return ctor("cpp:neg", {lift_expr(kids[0], ctx)});
    if (op == "+") return ctor("cpp:pos", {lift_expr(kids[0], ctx)});
    if (op == "!") return ctor("cpp:not", {lift_expr(kids[0], ctx)});
    if (op == "~") return ctor("cpp:bitnot", {lift_expr(kids[0], ctx)});
    if (op == "*") return ctor("cpp:deref", {lift_expr(kids[0], ctx)});
    if (op == "&") return ctor("cpp:addr", {lift_expr(kids[0], ctx)});
    if (op == "++" || op == "--") {
        add_write_effect_for_target(kids[0], ctx);
        return ctor((is_postfix_incdec(toks) ? (op == "++" ? "cpp:postinc" : "cpp:postdec") : (op == "++" ? "cpp:preinc" : "cpp:predec")), {lift_target(kids[0], ctx)});
    }
    throw unsupported(cursor, "unary operator '" + op + "' is not modeled");
}

ValuePtr lift_expr(CXCursor cursor, LiftContext& ctx) {
    CXCursorKind kind = clang_getCursorKind(cursor);
    switch (kind) {
        case CXCursor_IntegerLiteral:
            return int_const(parse_integer_literal(literal_token(cursor)));
        case CXCursor_FloatingLiteral:
            throw unsupported(cursor, "floating literals are not modeled by the draft C++ source lifter");
        case CXCursor_StringLiteral:
            return string_const(unquote_string_literal(literal_token(cursor)));
        case CXCursor_CharacterLiteral:
            return int_const(0);
        case CXCursor_CXXBoolLiteralExpr:
            return bool_const(literal_token(cursor) == "true");
        case CXCursor_CXXNullPtrLiteralExpr:
            return var_term("nullptr");
        case CXCursor_DeclRefExpr:
            return lift_decl_ref(cursor, ctx);
        case CXCursor_BinaryOperator:
        case CXCursor_CompoundAssignOperator:
            return lift_binary(cursor, ctx);
        case CXCursor_UnaryOperator:
            return lift_unary(cursor, ctx);
        case CXCursor_ParenExpr:
        case CXCursor_UnexposedExpr: {
            auto kids = children(cursor);
            if (kids.size() == 1) return lift_expr(kids.front(), ctx);
            throw unsupported(cursor, "wrapper expression with unexpected child count is not modeled");
        }
        case CXCursor_CStyleCastExpr:
        case CXCursor_CXXStaticCastExpr:
        case CXCursor_CXXConstCastExpr:
        case CXCursor_CXXReinterpretCastExpr:
        case CXCursor_CXXFunctionalCastExpr: {
            auto kids = children(cursor);
            if (kids.empty()) throw unsupported(cursor, "cast expression without operand is not modeled");
            return ctor("cpp:cast", {string_const(type_spelling(clang_getCursorType(cursor))), lift_expr(kids.back(), ctx)});
        }
        case CXCursor_CXXDynamicCastExpr:
            throw unsupported(cursor, "dynamic_cast/RTTI is not modeled");
        case CXCursor_CallExpr:
            return lift_call(cursor, ctx);
        case CXCursor_MemberRefExpr: {
            auto kids = children(cursor);
            if (kids.empty()) throw unsupported(cursor, "member reference without base is not modeled");
            return ctor("cpp:member", {lift_expr(kids.front(), ctx), string_const(cursor_spelling(cursor))});
        }
        case CXCursor_ArraySubscriptExpr: {
            auto kids = children(cursor);
            if (kids.size() != 2) throw unsupported(cursor, "array subscript with unexpected AST shape is not modeled");
            return ctor("cpp:index", {lift_expr(kids[0], ctx), lift_expr(kids[1], ctx)});
        }
        case CXCursor_ConditionalOperator: {
            auto kids = children(cursor);
            if (kids.size() != 3) throw unsupported(cursor, "ternary conditional with unexpected AST shape is not modeled");
            return ctor("cpp:ite", {lift_expr(kids[0], ctx), lift_expr(kids[1], ctx), lift_expr(kids[2], ctx)});
        }
        case CXCursor_CXXNewExpr: {
            std::vector<ValuePtr> args{string_const(type_spelling(clang_getCursorType(cursor)))};
            for (CXCursor child : children(cursor)) args.push_back(lift_expr(child, ctx));
            return ctor("cpp:new", args);
        }
        case CXCursor_CXXThrowExpr: {
            ctx.effects.add({"panics", "", "", ""});
            auto kids = children(cursor);
            return ctor("cpp:throw", {kids.empty() ? unit_const() : lift_expr(kids.front(), ctx)});
        }
        case CXCursor_LambdaExpr:
            throw unsupported(cursor, "lambda expressions are not modeled");
        case CXCursor_InitListExpr:
            throw unsupported(cursor, "initializer lists are not modeled");
        default:
            throw unsupported(cursor, "expression kind " + cursor_kind_name(cursor) + " is not modeled");
    }
}

ValuePtr lift_target(CXCursor cursor, LiftContext& ctx) {
    CXCursorKind kind = clang_getCursorKind(cursor);
    switch (kind) {
        case CXCursor_DeclRefExpr:
            return var_term(cursor_spelling(cursor));
        case CXCursor_MemberRefExpr: {
            auto kids = children(cursor);
            if (kids.empty()) throw unsupported(cursor, "member assignment target without base is not modeled");
            return ctor("cpp:member", {lift_expr(kids.front(), ctx), string_const(cursor_spelling(cursor))});
        }
        case CXCursor_ArraySubscriptExpr: {
            auto kids = children(cursor);
            if (kids.size() != 2) throw unsupported(cursor, "array assignment target with unexpected AST shape is not modeled");
            return ctor("cpp:index", {lift_expr(kids[0], ctx), lift_expr(kids[1], ctx)});
        }
        case CXCursor_UnaryOperator: {
            auto toks = cursor_tokens(cursor);
            auto kids = children(cursor);
            if (unary_prefix_op(toks) == "*" && kids.size() == 1) return ctor("cpp:deref", {lift_expr(kids.front(), ctx)});
            break;
        }
        case CXCursor_ParenExpr:
        case CXCursor_UnexposedExpr: {
            auto kids = children(cursor);
            if (kids.size() == 1) return lift_target(kids.front(), ctx);
            break;
        }
        default:
            break;
    }
    throw unsupported(cursor, "assignment target " + cursor_kind_name(cursor) + " is not modeled");
}

bool source_contains_token(CXCursor cursor, const std::string& token);

StmtResult lift_block(const std::vector<CXCursor>& stmts, LiftContext& ctx) {
    std::vector<ValuePtr> terms;
    ValuePtr last_return;
    bool has_return = false;
    for (CXCursor stmt : stmts) {
        StmtResult lifted = lift_stmt(stmt, ctx);
        terms.push_back(lifted.term);
        if (lifted.has_return) {
            last_return = lifted.return_term;
            has_return = true;
        }
    }
    return {seq_term(terms), last_return, has_return};
}

StmtResult lift_var_decl(CXCursor cursor, LiftContext& ctx) {
    std::string name = cursor_spelling(cursor);
    ctx.locals.insert(name);
    std::string usr = cursor_usr(cursor);
    if (!usr.empty()) ctx.local_usrs.insert(usr);
    auto kids = children(cursor);
    ValuePtr init = kids.empty() ? unit_const() : lift_expr(kids.back(), ctx);
    if (source_contains_token(cursor, "auto")) {
        throw unsupported(cursor, "auto local type deduction is not modeled");
    }
    if (!type_is_scalar_or_ref(clang_getCursorType(cursor))) {
        throw unsupported(cursor, "local object type " + type_spelling(clang_getCursorType(cursor)) + " may require destructor/RAII semantics and is not modeled");
    }
    return {ctor("cpp:decl", {string_const(name), init}), nullptr, false};
}

StmtResult lift_stmt(CXCursor cursor, LiftContext& ctx) {
    CXCursorKind kind = clang_getCursorKind(cursor);
    switch (kind) {
        case CXCursor_CompoundStmt:
            return lift_block(children(cursor), ctx);
        case CXCursor_ReturnStmt: {
            auto kids = children(cursor);
            if (kids.empty()) return {ctor("cpp:return", {unit_const()}), unit_const(), true};
            ValuePtr expr = lift_expr(kids.front(), ctx);
            return {ctor("cpp:return", {expr}), expr, true};
        }
        case CXCursor_DeclStmt: {
            std::vector<ValuePtr> terms;
            for (CXCursor child : children(cursor)) {
                if (clang_getCursorKind(child) != CXCursor_VarDecl) throw unsupported(child, "only local variable declarations are modeled in declaration statements");
                terms.push_back(lift_var_decl(child, ctx).term);
            }
            return {seq_term(terms), nullptr, false};
        }
        case CXCursor_IfStmt: {
            auto kids = children(cursor);
            if (kids.size() < 2 || kids.size() > 3) throw unsupported(cursor, "if statement with unexpected AST shape is not modeled");
            ValuePtr cond = lift_expr(kids[0], ctx);
            StmtResult then_branch = lift_stmt(kids[1], ctx);
            StmtResult else_branch = kids.size() == 3 ? lift_stmt(kids[2], ctx) : StmtResult{ctor("cpp:skip", {unit_const()}), nullptr, false};
            ValuePtr term = ctor("cpp:if", {cond, then_branch.term, else_branch.term});
            if (then_branch.has_return && else_branch.has_return) {
                return {term, ctor("cpp:ite", {cond, then_branch.return_term, else_branch.return_term}), true};
            }
            return {term, nullptr, false};
        }
        case CXCursor_WhileStmt: {
            auto kids = children(cursor);
            if (kids.size() != 2) throw unsupported(cursor, "while statement with unexpected AST shape is not modeled");
            ValuePtr term = ctor("cpp:while", {lift_expr(kids[0], ctx), lift_stmt(kids[1], ctx).term});
            ctx.effects.add({"opaque_loop", "", "", cid_of_value(term)});
            return {term, nullptr, false};
        }
        case CXCursor_DoStmt: {
            auto kids = children(cursor);
            if (kids.size() != 2) throw unsupported(cursor, "do statement with unexpected AST shape is not modeled");
            ValuePtr term = ctor("cpp:do", {lift_stmt(kids[0], ctx).term, lift_expr(kids[1], ctx)});
            ctx.effects.add({"opaque_loop", "", "", cid_of_value(term)});
            return {term, nullptr, false};
        }
        case CXCursor_ForStmt: {
            auto kids = children(cursor);
            if (kids.size() < 4) throw unsupported(cursor, "for statement with unexpected AST shape is not modeled");
            StmtResult init = lift_stmt(kids[0], ctx);
            ValuePtr cond = lift_expr(kids[1], ctx);
            ValuePtr update = lift_expr(kids[2], ctx);
            StmtResult body = lift_stmt(kids[3], ctx);
            ValuePtr term = ctor("cpp:for", {init.term, cond, update, body.term});
            ctx.effects.add({"opaque_loop", "", "", cid_of_value(term)});
            return {term, nullptr, false};
        }
        case CXCursor_BreakStmt:
            return {ctor("cpp:break", {unit_const()}), nullptr, false};
        case CXCursor_ContinueStmt:
            return {ctor("cpp:continue", {unit_const()}), nullptr, false};
        case CXCursor_NullStmt:
            return {ctor("cpp:skip", {unit_const()}), nullptr, false};
        case CXCursor_CXXThrowExpr: {
            ValuePtr term = lift_expr(cursor, ctx);
            return {term, nullptr, false};
        }
        case CXCursor_BinaryOperator:
        case CXCursor_CompoundAssignOperator:
        case CXCursor_UnaryOperator:
        case CXCursor_CallExpr:
        case CXCursor_UnexposedExpr:
        case CXCursor_ParenExpr:
        case CXCursor_DeclRefExpr:
            return {lift_expr(cursor, ctx), nullptr, false};
        case CXCursor_CXXTryStmt:
            throw unsupported(cursor, "try/catch exception handling is not modeled");
        case CXCursor_LambdaExpr:
            throw unsupported(cursor, "lambda expressions are not modeled");
        default:
            throw unsupported(cursor, "statement kind " + cursor_kind_name(cursor) + " is not modeled");
    }
}

struct TranslationUnitHandle {
    CXIndex index = nullptr;
    CXTranslationUnit unit = nullptr;
    ~TranslationUnitHandle() {
        if (unit) clang_disposeTranslationUnit(unit);
        if (index) clang_disposeIndex(index);
    }
};

TranslationUnitHandle parse_translation_unit(const std::string& path, const std::string& source, std::vector<ValuePtr>& diagnostics) {
    TranslationUnitHandle handle;
    handle.index = clang_createIndex(0, 0);
    CXUnsavedFile unsaved;
    unsaved.Filename = path.c_str();
    unsaved.Contents = source.c_str();
    unsaved.Length = static_cast<unsigned long>(source.size());
    const char* args[] = {"-x", "c++", "-std=c++20"};
    CXErrorCode err = clang_parseTranslationUnit2(
        handle.index,
        path.c_str(),
        args,
        3,
        &unsaved,
        1,
        CXTranslationUnit_DetailedPreprocessingRecord | CXTranslationUnit_SkipFunctionBodies,
        &handle.unit);
    if (err != CXError_Success || !handle.unit) {
        throw std::runtime_error("libclang failed to parse translation unit");
    }
    unsigned n = clang_getNumDiagnostics(handle.unit);
    for (unsigned i = 0; i < n; ++i) {
        CXDiagnostic diag = clang_getDiagnostic(handle.unit, i);
        CXDiagnosticSeverity severity = clang_getDiagnosticSeverity(diag);
        if (severity >= CXDiagnostic_Error) {
            diagnostics.push_back(obj({{"message", strv(cx_string(clang_formatDiagnostic(diag, clang_defaultDiagnosticDisplayOptions())))}, {"severity", strv("error")}}));
        }
        clang_disposeDiagnostic(diag);
    }
    clang_disposeTranslationUnit(handle.unit);
    handle.unit = nullptr;

    err = clang_parseTranslationUnit2(
        handle.index,
        path.c_str(),
        args,
        3,
        &unsaved,
        1,
        CXTranslationUnit_DetailedPreprocessingRecord,
        &handle.unit);
    if (err != CXError_Success || !handle.unit) {
        throw std::runtime_error("libclang failed to parse translation unit bodies");
    }
    return handle;
}

struct CollectContext {
    std::set<std::string> known_function_usrs;
    std::set<std::string> global_var_usrs;
    std::map<std::string, std::string> global_cells;
};

void collect_known(CXCursor root, CollectContext& ctx) {
    clang_visitChildren(
        root,
        [](CXCursor cursor, CXCursor, CXClientData data) {
            auto* ctx = static_cast<CollectContext*>(data);
            if (!from_main_file(cursor)) return CXChildVisit_Continue;
            CXCursorKind kind = clang_getCursorKind(cursor);
            if (is_function_cursor(kind) && clang_isCursorDefinition(cursor)) {
                std::string usr = cursor_usr(cursor);
                if (!usr.empty()) ctx->known_function_usrs.insert(usr);
                return CXChildVisit_Continue;
            }
            if (cursor_is_global_var(cursor)) {
                std::string usr = cursor_usr(cursor);
                if (!usr.empty()) {
                    ctx->global_var_usrs.insert(usr);
                    std::string q = qualified_name(cursor);
                    ctx->global_cells[usr] = q.empty() ? cursor_spelling(cursor) : q;
                }
            }
            if (kind == CXCursor_FunctionTemplate || kind == CXCursor_ClassTemplate) return CXChildVisit_Continue;
            return CXChildVisit_Recurse;
        },
        &ctx);
}

ValuePtr template_with_param_refs(const ValuePtr& value, const std::map<std::string, int>& param_index) {
    if (!value) return nullv();
    switch (value->kind()) {
        case ValueKind::Null:
            return nullv();
        case ValueKind::Bool:
            return boolv(value->as_bool());
        case ValueKind::Integer:
            return intv(value->as_int());
        case ValueKind::String:
            return strv(value->as_string());
        case ValueKind::Array: {
            std::vector<ValuePtr> items;
            for (const auto& item : value->as_array()) items.push_back(template_with_param_refs(item, param_index));
            return arr(items);
        }
        case ValueKind::Object: {
            std::string kind = get_string(get_field(value, "kind"));
            if (kind == "var") {
                std::string name = get_string(get_field(value, "name"));
                auto it = param_index.find(name);
                if (it != param_index.end()) {
                    return obj({{"index", intv(it->second)}, {"kind", strv("param_ref")}});
                }
            }
            std::vector<std::pair<std::string, ValuePtr>> fields;
            for (const auto& [key, child] : value->as_object()) {
                fields.push_back({key, template_with_param_refs(child, param_index)});
            }
            return obj(fields);
        }
    }
    return nullv();
}

ValuePtr ast_template_for_body(const ValuePtr& body_term, const std::vector<std::string>& formals) {
    std::map<std::string, int> param_index;
    for (size_t i = 0; i < formals.size(); ++i) {
        param_index.emplace(formals[i], static_cast<int>(i + 1));
    }
    return template_with_param_refs(body_term, param_index);
}

std::string body_text_from_compound(CXCursor body, const std::string& source) {
    auto offsets = cursor_offsets(body);
    if (!offsets) return trim_ws(cursor_source(body));
    size_t start = offsets->first;
    size_t end = offsets->second;
    if (start > source.size()) return "";
    if (end > source.size()) end = source.size();
    if (end < start) return "";
    std::string extent = source.substr(start, end - start);
    size_t open = extent.find('{');
    size_t close = extent.rfind('}');
    if (open != std::string::npos && close != std::string::npos && close > open) {
        return trim_ws(extent.substr(open + 1, close - open - 1));
    }
    return trim_ws(extent);
}

ValuePtr body_source_value(CXCursor body,
                           const std::string& path,
                           const std::string& source,
                           const ValuePtr& body_term,
                           const std::vector<std::string>& formals) {
    ValuePtr ast_template = ast_template_for_body(body_term, formals);
    std::string body_text = body_text_from_compound(body, source);
    std::vector<ValuePtr> param_names;
    for (const auto& formal : formals) param_names.push_back(strv(formal));
    return obj({
        {"ast_template", ast_template},
        {"body_text", strv(body_text)},
        {"file", strv(path)},
        {"param_names", arr(param_names)},
        {"source_cid", strv(canonicalizer::compute_cid(body_text))},
        {"span", span_value(cursor_span(body))},
        {"template_cid", strv(cid_of_value(ast_template))},
    });
}

ValuePtr function_contract(CXCursor fn, LiftContext& ctx, ValuePtr body_term, ValuePtr post_term, ValuePtr body_source, const std::vector<std::string>& formals, const std::vector<ValuePtr>& formal_sorts, ValuePtr return_sort) {
    SourceLoc loc = cursor_loc(fn);
    std::string body_cid = cid_of_value(body_term);
    return obj({
        {"autoMintedMementos", empty_array()},
        {"bodyCid", strv(body_cid)},
        {"body_source", std::move(body_source)},
        {"effects", arr(ctx.effects.values())},
        {"fnName", strv(ctx.function_name)},
        {"formalSorts", arr(formal_sorts)},
        {"formals", [&] { std::vector<ValuePtr> names; for (const auto& f : formals) names.push_back(strv(f)); return arr(names); }()},
        {"kind", strv("function-contract")},
        {"locus", locus_value(ctx.path, loc.line, loc.col)},
        {"post", eq_formula(var_term("return_value"), post_term)},
        {"pre", true_formula()},
        {"returnSort", std::move(return_sort)},
        {"schemaVersion", strv("1")},
    });
}

bool source_contains_token(CXCursor cursor, const std::string& token) {
    for (const auto& t : cursor_tokens(cursor)) {
        if (t == token) return true;
    }
    return false;
}

bool unsupported_function_shape(CXCursor fn, std::string& reason) {
    std::string spelling = cursor_spelling(fn);
    if (spelling.find("operator") == 0 || spelling == "operator") {
        reason = "operator overloading is not modeled";
        return true;
    }
    CXType ret = clang_getCursorResultType(fn);
    std::string ret_text = type_spelling(ret);
    if (ret.kind == CXType_Auto || ret_text == "auto") {
        reason = "auto return type deduction is not modeled";
        return true;
    }
    if (!type_is_scalar_or_ref(ret)) {
        reason = "only single scalar/ref return types are modeled, got " + ret_text;
        return true;
    }
    if (source_contains_token(fn, "constexpr")) {
        reason = "constexpr evaluation-dependent code is not modeled";
        return true;
    }
    int argc = clang_Cursor_getNumArguments(fn);
    for (int i = 0; i < argc; ++i) {
        CXCursor arg = clang_Cursor_getArgument(fn, static_cast<unsigned>(i));
        CXType type = clang_getCursorType(arg);
        if (type.kind == CXType_Auto || type_spelling(type) == "auto") {
            reason = "auto parameter deduction is not modeled";
            return true;
        }
        if (!type_is_scalar_or_ref(type)) {
            reason = "only scalar/ref parameters are modeled, got " + type_spelling(type);
            return true;
        }
        if (cursor_spelling(arg).empty()) {
            reason = "unnamed parameters are refused to keep formals deterministic";
            return true;
        }
    }
    return false;
}

void lift_function(CXCursor fn, const CollectContext& collected, const std::string& path, const std::string& source, std::vector<ValuePtr>& declarations, std::vector<ValuePtr>& body_terms, std::vector<Refusal>& refusals) {
    std::string fn_name = stable_function_name(fn);
    std::string shape_reason;
    if (unsupported_function_shape(fn, shape_reason)) {
        refusals.push_back({cursor_kind_name(fn), fn_name, cursor_loc(fn).line, shape_reason});
        return;
    }

    std::vector<std::string> formals;
    std::vector<ValuePtr> formal_sorts;
    LiftContext ctx;
    ctx.path = path;
    ctx.known_function_usrs = collected.known_function_usrs;
    ctx.global_var_usrs = collected.global_var_usrs;
    ctx.global_cells = collected.global_cells;
    ctx.function_name = fn_name;

    int argc = clang_Cursor_getNumArguments(fn);
    for (int i = 0; i < argc; ++i) {
        CXCursor arg = clang_Cursor_getArgument(fn, static_cast<unsigned>(i));
        std::string name = cursor_spelling(arg);
        formals.push_back(name);
        formal_sorts.push_back(prim_sort(sort_name_for_type(clang_getCursorType(arg))));
        ctx.locals.insert(name);
        std::string usr = cursor_usr(arg);
        if (!usr.empty()) ctx.local_usrs.insert(usr);
    }

    std::vector<CXCursor> kids = children(fn);
    CXCursor body{};
    bool found_body = false;
    for (CXCursor child : kids) {
        if (clang_getCursorKind(child) == CXCursor_CompoundStmt) {
            body = child;
            found_body = true;
            break;
        }
    }
    if (!found_body) {
        refusals.push_back({cursor_kind_name(fn), fn_name, cursor_loc(fn).line, "function definition has no compound body"});
        return;
    }

    try {
        StmtResult lifted = lift_stmt(body, ctx);
        CXType ret = clang_getCursorResultType(fn);
        bool returns_unit = clang_getCanonicalType(ret).kind == CXType_Void;
        if (!returns_unit && !lifted.has_return) {
            refusals.push_back({cursor_kind_name(fn), fn_name, cursor_loc(fn).line, "non-void function has no modeled return"});
            return;
        }
        ValuePtr post = lifted.has_return ? lifted.return_term : unit_const();
        ValuePtr body_source = body_source_value(body, path, source, lifted.term, formals);
        declarations.push_back(function_contract(fn, ctx, lifted.term, post, body_source, formals, formal_sorts, prim_sort(sort_name_for_type(ret))));
        body_terms.push_back(lifted.term);
    } catch (const Unsupported& u) {
        refusals.push_back({u.kind, fn_name, u.line, u.what()});
    } catch (const std::exception& ex) {
        refusals.push_back({"analysis-error", fn_name, cursor_loc(fn).line, ex.what()});
    }
}

struct LiftVisitContext {
    const CollectContext* collected = nullptr;
    std::string path;
    std::string source;
    std::vector<ValuePtr>* declarations = nullptr;
    std::vector<ValuePtr>* body_terms = nullptr;
    std::vector<Refusal>* refusals = nullptr;
};

void lift_translation_unit(CXCursor root, LiftVisitContext& ctx) {
    clang_visitChildren(
        root,
        [](CXCursor cursor, CXCursor, CXClientData data) {
            auto* ctx = static_cast<LiftVisitContext*>(data);
            if (!from_main_file(cursor)) return CXChildVisit_Continue;
            CXCursorKind kind = clang_getCursorKind(cursor);
            if (kind == CXCursor_FunctionTemplate || kind == CXCursor_ClassTemplate) {
                ctx->refusals->push_back({cursor_kind_name(cursor), cursor_spelling(cursor), cursor_loc(cursor).line, "templates are not modeled by the C++ source lifter"});
                return CXChildVisit_Continue;
            }
            if (is_unsupported_definition_cursor(kind) && clang_isCursorDefinition(cursor)) {
                ctx->refusals->push_back({cursor_kind_name(cursor), stable_function_name(cursor), cursor_loc(cursor).line, "constructors, destructors, and conversion functions are not modeled"});
                return CXChildVisit_Continue;
            }
            if (is_function_cursor(kind) && clang_isCursorDefinition(cursor)) {
                lift_function(cursor, *ctx->collected, ctx->path, ctx->source, *ctx->declarations, *ctx->body_terms, *ctx->refusals);
                return CXChildVisit_Continue;
            }
            return CXChildVisit_Recurse;
        },
        &ctx);
}

ValuePtr source_unit_contract(const std::string& path, const std::string& source, const std::vector<ValuePtr>& body_terms) {
    ValuePtr body = seq_term(body_terms);
    ValuePtr source_term = ctor("cpp:source-unit", {bytes_term(source), body});
    return obj({
        {"autoMintedMementos", empty_array()},
        {"bodyCid", nullv()},
        {"effects", empty_array()},
        {"fnName", strv("<source-unit:" + path + ">")},
        {"formalSorts", empty_array()},
        {"formals", empty_array()},
        {"kind", strv("function-contract")},
        {"locus", locus_value(path, 1, 1)},
        {"post", eq_formula(var_term("return_value"), source_term)},
        {"pre", true_formula()},
        {"returnSort", prim_sort("Unit")},
        {"schemaVersion", strv("1")},
    });
}

std::string escape_source_string(const std::string& s) {
    std::ostringstream out;
    out << '"';
    for (char ch : s) {
        switch (ch) {
            case '\\': out << "\\\\"; break;
            case '"': out << "\\\""; break;
            case '\n': out << "\\n"; break;
            case '\r': out << "\\r"; break;
            case '\t': out << "\\t"; break;
            default: out << ch; break;
        }
    }
    out << '"';
    return out.str();
}

std::string string_value_from_term(const ValuePtr& term) {
    if (get_string(get_field(term, "kind")) == "const") return get_string(get_field(term, "value"));
    return get_string(get_field(term, "name"));
}

std::string expr_from_term(const ValuePtr& term);

std::string binary_source(const std::vector<ValuePtr>& args, const std::string& op) {
    if (args.size() != 2) throw std::runtime_error("binary term arity mismatch");
    return "(" + expr_from_term(args[0]) + " " + op + " " + expr_from_term(args[1]) + ")";
}

std::string unary_source(const std::vector<ValuePtr>& args, const std::string& op) {
    if (args.size() != 1) throw std::runtime_error("unary term arity mismatch");
    return "(" + op + expr_from_term(args[0]) + ")";
}

std::string expr_from_term(const ValuePtr& term) {
    std::string kind = get_string(get_field(term, "kind"));
    if (kind == "var") return get_string(get_field(term, "name"));
    if (kind == "const") {
        auto value = get_field(term, "value");
        if (!value) return "0";
        if (value->kind() == ValueKind::Integer) return std::to_string(value->as_int());
        if (value->kind() == ValueKind::Bool) return value->as_bool() ? "true" : "false";
        if (value->kind() == ValueKind::String) return escape_source_string(value->as_string());
        if (value->kind() == ValueKind::Null) return "nullptr";
        return "0";
    }
    if (kind != "ctor") return "0";
    std::string name = get_string(get_field(term, "name"));
    std::vector<ValuePtr> args = term_args(term);
    if (name == "cpp:add") return binary_source(args, "+");
    if (name == "cpp:sub") return binary_source(args, "-");
    if (name == "cpp:mul") return binary_source(args, "*");
    if (name == "cpp:div") return binary_source(args, "/");
    if (name == "cpp:mod") return binary_source(args, "%");
    if (name == "cpp:eq") return binary_source(args, "==");
    if (name == "cpp:ne") return binary_source(args, "!=");
    if (name == "cpp:lt") return binary_source(args, "<");
    if (name == "cpp:le") return binary_source(args, "<=");
    if (name == "cpp:gt") return binary_source(args, ">");
    if (name == "cpp:ge") return binary_source(args, ">=");
    if (name == "cpp:and") return binary_source(args, "&&");
    if (name == "cpp:or") return binary_source(args, "||");
    if (name == "cpp:bitand") return binary_source(args, "&");
    if (name == "cpp:bitor") return binary_source(args, "|");
    if (name == "cpp:bitxor") return binary_source(args, "^");
    if (name == "cpp:shl") return binary_source(args, "<<");
    if (name == "cpp:shr") return binary_source(args, ">>");
    if (name == "cpp:neg") return unary_source(args, "-");
    if (name == "cpp:pos") return unary_source(args, "+");
    if (name == "cpp:not") return unary_source(args, "!");
    if (name == "cpp:bitnot") return unary_source(args, "~");
    if (name == "cpp:deref") return unary_source(args, "*");
    if (name == "cpp:addr") return unary_source(args, "&");
    if (name == "cpp:preinc" && args.size() == 1) return "(++" + expr_from_term(args[0]) + ")";
    if (name == "cpp:predec" && args.size() == 1) return "(--" + expr_from_term(args[0]) + ")";
    if (name == "cpp:postinc" && args.size() == 1) return "(" + expr_from_term(args[0]) + "++)";
    if (name == "cpp:postdec" && args.size() == 1) return "(" + expr_from_term(args[0]) + "--)";
    if (name == "cpp:index" && args.size() == 2) return expr_from_term(args[0]) + "[" + expr_from_term(args[1]) + "]";
    if (name == "cpp:member" && args.size() == 2) return expr_from_term(args[0]) + "." + string_value_from_term(args[1]);
    if (name == "cpp:ite" && args.size() == 3) return "(" + expr_from_term(args[0]) + " ? " + expr_from_term(args[1]) + " : " + expr_from_term(args[2]) + ")";
    if (name == "cpp:assign" && args.size() == 2) return "(" + expr_from_term(args[0]) + " = " + expr_from_term(args[1]) + ")";
    if (name == "cpp:call" && !args.empty()) {
        std::string callee = string_value_from_term(args[0]);
        std::vector<std::string> parts;
        for (size_t i = 1; i < args.size(); ++i) parts.push_back(expr_from_term(args[i]));
        std::ostringstream out;
        out << callee << "(";
        for (size_t i = 0; i < parts.size(); ++i) {
            if (i > 0) out << ", ";
            out << parts[i];
        }
        out << ")";
        return out.str();
    }
    if (name == "cpp:cast" && args.size() == 2) return "(" + string_value_from_term(args[0]) + ")(" + expr_from_term(args[1]) + ")";
    if (name == "cpp:new" && !args.empty()) return "new " + string_value_from_term(args[0]) + "()";
    if (name == "cpp:return" && !args.empty()) return expr_from_term(args[0]);
    return "0";
}

void emit_stmt(const ValuePtr& term, std::vector<std::string>& lines, int indent);

std::string ind(int indent) { return std::string(static_cast<size_t>(indent) * 4, ' '); }

std::string for_header_init_from_term(const ValuePtr& term) {
    if (is_kind_name(term, "ctor", "cpp:decl")) {
        std::vector<ValuePtr> args = term_args(term);
        if (args.size() == 2) return "int " + string_value_from_term(args[0]) + " = " + expr_from_term(args[1]);
    }
    if (is_kind_name(term, "ctor", "cpp:skip")) return "";
    return expr_from_term(term);
}

void emit_stmt(const ValuePtr& term, std::vector<std::string>& lines, int indent) {
    if (!term || get_string(get_field(term, "kind")) != "ctor") {
        lines.push_back(ind(indent) + expr_from_term(term) + ";");
        return;
    }
    std::string name = get_string(get_field(term, "name"));
    std::vector<ValuePtr> args = term_args(term);
    if (name == "cpp:seq") {
        for (const auto& arg : args) emit_stmt(arg, lines, indent);
    } else if (name == "cpp:return") {
        lines.push_back(ind(indent) + "return " + (args.empty() ? std::string{} : expr_from_term(args[0])) + ";");
    } else if (name == "cpp:decl" && args.size() == 2) {
        lines.push_back(ind(indent) + "int " + string_value_from_term(args[0]) + " = " + expr_from_term(args[1]) + ";");
    } else if (name == "cpp:assign") {
        lines.push_back(ind(indent) + expr_from_term(term) + ";");
    } else if (name == "cpp:if" && args.size() == 3) {
        lines.push_back(ind(indent) + "if (" + expr_from_term(args[0]) + ") {");
        emit_stmt(args[1], lines, indent + 1);
        lines.push_back(ind(indent) + "} else {");
        emit_stmt(args[2], lines, indent + 1);
        lines.push_back(ind(indent) + "}");
    } else if (name == "cpp:while" && args.size() == 2) {
        lines.push_back(ind(indent) + "while (" + expr_from_term(args[0]) + ") {");
        emit_stmt(args[1], lines, indent + 1);
        lines.push_back(ind(indent) + "}");
    } else if (name == "cpp:for" && args.size() == 4) {
        lines.push_back(ind(indent) + "for (" + for_header_init_from_term(args[0]) + "; " + expr_from_term(args[1]) + "; " + expr_from_term(args[2]) + ") {");
        emit_stmt(args[3], lines, indent + 1);
        lines.push_back(ind(indent) + "}");
    } else if (name == "cpp:skip") {
        lines.push_back(ind(indent) + ";");
    } else if (name == "cpp:throw") {
        lines.push_back(ind(indent) + "throw " + (args.empty() ? std::string("0") : expr_from_term(args[0])) + ";");
    } else {
        lines.push_back(ind(indent) + expr_from_term(term) + ";");
    }
}

std::vector<std::string> free_vars(const ValuePtr& term) {
    std::set<std::string> seen;
    std::vector<std::string> out;
    std::function<void(ValuePtr)> visit = [&](ValuePtr node) {
        if (!node || node->kind() != ValueKind::Object) return;
        std::string kind = get_string(get_field(node, "kind"));
        if (kind == "var") {
            std::string name = get_string(get_field(node, "name"));
            if (name != "return_value" && name != "nullptr" && !seen.count(name)) {
                seen.insert(name);
                out.push_back(name);
            }
        }
        auto args = get_field(node, "args");
        if (args && args->kind() == ValueKind::Array) {
            for (const auto& arg : args->as_array()) visit(arg);
        }
    };
    visit(term);
    return out;
}

std::string decode_hex_bytes(const std::string& hex) {
    auto nibble = [](char c) -> int {
        if (c >= '0' && c <= '9') return c - '0';
        if (c >= 'a' && c <= 'f') return c - 'a' + 10;
        if (c >= 'A' && c <= 'F') return c - 'A' + 10;
        return -1;
    };
    std::string out;
    for (size_t i = 0; i + 1 < hex.size(); i += 2) {
        int hi = nibble(hex[i]);
        int lo = nibble(hex[i + 1]);
        if (hi < 0 || lo < 0) return "";
        out.push_back(static_cast<char>((hi << 4) | lo));
    }
    return out;
}

}  // namespace

std::string canonical_bytes(const ValuePtr& value) {
    return canonicalizer::encode_jcs(value);
}

std::string cid_of_value(const ValuePtr& value) {
    return canonicalizer::compute_cid(canonical_bytes(value));
}

ValuePtr initialize_result() {
    return obj({
        {"capabilities", obj({{"authoring_surfaces", arr({strv(DIALECT)})}, {"emits_signed_mementos", boolv(false)}, {"ir_version", strv(IR_VERSION)}})},
        {"dialect", strv(DIALECT)},
        {"name", strv("provekit-lift-cpp-source")},
        {"protocol_version", strv("pep/1.7.0")},
        {"version", strv(VERSION)},
    });
}

LiftResult lift_source(const std::string& path, const std::string& source) {
    LiftResult result;
    TranslationUnitHandle tu = parse_translation_unit(path, source, result.diagnostics);
    CollectContext collected;
    collect_known(clang_getTranslationUnitCursor(tu.unit), collected);
    std::vector<ValuePtr> function_decls;
    std::vector<ValuePtr> body_terms;
    LiftVisitContext ctx;
    ctx.collected = &collected;
    ctx.path = path;
    ctx.source = source;
    ctx.declarations = &function_decls;
    ctx.body_terms = &body_terms;
    ctx.refusals = &result.refusals;
    lift_translation_unit(clang_getTranslationUnitCursor(tu.unit), ctx);
    if (!function_decls.empty()) {
        result.declarations.push_back(source_unit_contract(path, source, body_terms));
        result.declarations.insert(result.declarations.end(), function_decls.begin(), function_decls.end());
    }
    return result;
}

LiftResult lift_paths(const std::string& workspace_root, const std::vector<std::string>& source_paths) {
    LiftResult aggregate;
    for (const auto& rel : source_paths) {
        std::string path = workspace_root;
        if (!path.empty() && path.back() != '/') path.push_back('/');
        path += rel;
        std::ifstream in(path, std::ios::binary);
        if (!in) {
            aggregate.diagnostics.push_back(obj({{"message", strv("path not found: " + path)}, {"severity", strv("warning")}}));
            continue;
        }
        std::ostringstream buf;
        buf << in.rdbuf();
        LiftResult lifted = lift_source(rel, buf.str());
        aggregate.declarations.insert(aggregate.declarations.end(), lifted.declarations.begin(), lifted.declarations.end());
        aggregate.refusals.insert(aggregate.refusals.end(), lifted.refusals.begin(), lifted.refusals.end());
        aggregate.diagnostics.insert(aggregate.diagnostics.end(), lifted.diagnostics.begin(), lifted.diagnostics.end());
    }
    return aggregate;
}

ValuePtr post_rhs(const ValuePtr& contract) {
    ValuePtr post = get_field(contract, "post");
    if (!post) throw std::runtime_error("contract has no post field");
    auto args = get_field(post, "args");
    if (!args || args->kind() != ValueKind::Array || args->as_array().size() != 2) {
        throw std::runtime_error("contract post is not a two-argument equality");
    }
    return args->as_array()[1];
}

const ValuePtr* find_contract(const LiftResult& result, const std::string& name_fragment) {
    for (const auto& item : result.declarations) {
        if (get_string(get_field(item, "kind")) != "function-contract") continue;
        std::string name = get_string(get_field(item, "fnName"));
        if (name.find("<source-unit:") != std::string::npos) continue;
        if (name.find(name_fragment) != std::string::npos) return &item;
    }
    return nullptr;
}

std::string compile_body_term(const ValuePtr& term, const CompileBodyOptions& options) {
    std::vector<std::string> formals = options.formals.empty() ? free_vars(term) : options.formals;
    std::vector<std::string> lines;
    lines.push_back(options.return_type + " " + options.function_name + "(");
    std::ostringstream sig;
    sig << options.return_type << " " << options.function_name << "(";
    for (size_t i = 0; i < formals.size(); ++i) {
        if (i > 0) sig << ", ";
        sig << "int " << formals[i];
    }
    sig << ") {";
    std::vector<std::string> body;
    if (is_kind_name(term, "ctor", "cpp:return") || is_kind_name(term, "ctor", "cpp:seq") || is_kind_name(term, "ctor", "cpp:if") ||
        is_kind_name(term, "ctor", "cpp:while") || is_kind_name(term, "ctor", "cpp:for")) {
        emit_stmt(term, body, 1);
    } else {
        body.push_back("    return " + expr_from_term(term) + ";");
    }
    std::ostringstream out;
    out << sig.str() << "\n";
    for (const auto& line : body) out << line << "\n";
    out << "}\n";
    return out.str();
}

std::string compile_ir_document(const std::vector<ValuePtr>& ir) {
    for (const auto& item : ir) {
        if (get_string(get_field(item, "kind")) != "function-contract") continue;
        ValuePtr rhs = post_rhs(item);
        if (!is_kind_name(rhs, "ctor", "cpp:source-unit")) continue;
        auto args = term_args(rhs);
        if (!args.empty() && get_string(get_field(args[0], "kind")) == "bytes") {
            return decode_hex_bytes(get_string(get_field(args[0], "value")));
        }
    }
    std::ostringstream out;
    for (const auto& item : ir) {
        if (get_string(get_field(item, "kind")) != "function-contract") continue;
        std::string fn = get_string(get_field(item, "fnName"));
        if (fn.find("<source-unit:") != std::string::npos) continue;
        CompileBodyOptions options;
        options.function_name = "lifted";
        if (auto formals = get_field(item, "formals"); formals && formals->kind() == ValueKind::Array) {
            for (const auto& formal : formals->as_array()) options.formals.push_back(get_string(formal));
        }
        out << compile_body_term(post_rhs(item), options) << "\n";
    }
    return out.str();
}

std::string lift_result_json(const LiftResult& result) {
    std::vector<ValuePtr> refusals;
    for (const auto& r : result.refusals) {
        std::vector<std::pair<std::string, ValuePtr>> fields{{"kind", strv(r.kind)}, {"reason", strv(r.reason)}};
        if (!r.function.empty()) fields.push_back({"function", strv(r.function)});
        if (r.line > 0) fields.push_back({"line", intv(r.line)});
        refusals.push_back(obj(fields));
    }
    ValuePtr response = obj({
        {"callEdges", empty_array()},
        {"diagnostics", arr(result.diagnostics)},
        {"ir", arr(result.declarations)},
        {"kind", strv("ir-document")},
        {"opacityReport", arr(result.opacity_report)},
        {"refusals", arr(refusals)},
    });
    return canonical_bytes(response);
}

namespace {

class JsonParser {
   public:
    explicit JsonParser(std::string text) : text_(std::move(text)) {}

    ValuePtr parse() {
        skip_ws();
        ValuePtr value = parse_value();
        skip_ws();
        return value;
    }

   private:
    ValuePtr parse_value() {
        skip_ws();
        if (pos_ >= text_.size()) throw std::runtime_error("unexpected end of JSON");
        char ch = text_[pos_];
        if (ch == '"') return strv(parse_string());
        if (ch == '{') return parse_object();
        if (ch == '[') return parse_array();
        if (ch == 't' && consume("true")) return boolv(true);
        if (ch == 'f' && consume("false")) return boolv(false);
        if (ch == 'n' && consume("null")) return nullv();
        return parse_number();
    }

    ValuePtr parse_object() {
        expect('{');
        std::vector<std::pair<std::string, ValuePtr>> fields;
        skip_ws();
        if (peek('}')) {
            ++pos_;
            return obj(fields);
        }
        while (true) {
            skip_ws();
            std::string key = parse_string();
            skip_ws();
            expect(':');
            fields.push_back({key, parse_value()});
            skip_ws();
            if (peek('}')) {
                ++pos_;
                break;
            }
            expect(',');
        }
        return obj(fields);
    }

    ValuePtr parse_array() {
        expect('[');
        std::vector<ValuePtr> values;
        skip_ws();
        if (peek(']')) {
            ++pos_;
            return arr(values);
        }
        while (true) {
            values.push_back(parse_value());
            skip_ws();
            if (peek(']')) {
                ++pos_;
                break;
            }
            expect(',');
        }
        return arr(values);
    }

    ValuePtr parse_number() {
        size_t start = pos_;
        if (text_[pos_] == '-') ++pos_;
        while (pos_ < text_.size() && std::isdigit(static_cast<unsigned char>(text_[pos_]))) ++pos_;
        if (pos_ < text_.size() && (text_[pos_] == '.' || text_[pos_] == 'e' || text_[pos_] == 'E')) {
            throw std::runtime_error("floating JSON numbers are not supported by cpp-source RPC parser");
        }
        return intv(std::stoll(text_.substr(start, pos_ - start)));
    }

    std::string parse_string() {
        expect('"');
        std::string out;
        while (pos_ < text_.size()) {
            char ch = text_[pos_++];
            if (ch == '"') return out;
            if (ch == '\\') {
                if (pos_ >= text_.size()) throw std::runtime_error("bad JSON escape");
                char esc = text_[pos_++];
                switch (esc) {
                    case '"': out.push_back('"'); break;
                    case '\\': out.push_back('\\'); break;
                    case '/': out.push_back('/'); break;
                    case 'b': out.push_back('\b'); break;
                    case 'f': out.push_back('\f'); break;
                    case 'n': out.push_back('\n'); break;
                    case 'r': out.push_back('\r'); break;
                    case 't': out.push_back('\t'); break;
                    case 'u': {
                        if (pos_ + 4 > text_.size()) throw std::runtime_error("bad unicode escape");
                        std::string hex = text_.substr(pos_, 4);
                        pos_ += 4;
                        int code = std::strtol(hex.c_str(), nullptr, 16);
                        if (code <= 0x7f) out.push_back(static_cast<char>(code));
                        break;
                    }
                    default:
                        throw std::runtime_error("bad JSON escape");
                }
            } else {
                out.push_back(ch);
            }
        }
        throw std::runtime_error("unterminated JSON string");
    }

    bool consume(const char* word) {
        size_t len = std::char_traits<char>::length(word);
        if (text_.compare(pos_, len, word) != 0) return false;
        pos_ += len;
        return true;
    }

    bool peek(char ch) const { return pos_ < text_.size() && text_[pos_] == ch; }
    void expect(char ch) {
        skip_ws();
        if (!peek(ch)) throw std::runtime_error("unexpected JSON token");
        ++pos_;
    }
    void skip_ws() {
        while (pos_ < text_.size() && std::isspace(static_cast<unsigned char>(text_[pos_]))) ++pos_;
    }

    std::string text_;
    size_t pos_ = 0;
};

std::string response(ValuePtr id, ValuePtr result) {
    return canonical_bytes(obj({{"id", id ? id : nullv()}, {"jsonrpc", strv("2.0")}, {"result", result ? result : nullv()}}));
}

std::string error_response(ValuePtr id, int code, const std::string& message) {
    return canonical_bytes(obj({{"error", obj({{"code", intv(code)}, {"message", strv(message)}})}, {"id", id ? id : nullv()}, {"jsonrpc", strv("2.0")}}));
}

ValuePtr request_id(const ValuePtr& req) { return get_field(req, "id") ? get_field(req, "id") : nullv(); }

std::vector<std::string> source_paths_from_params(const ValuePtr& params) {
    std::vector<std::string> paths;
    auto node = get_field(params, "source_paths");
    if (!node || node->kind() != ValueKind::Array) return paths;
    for (const auto& item : node->as_array()) paths.push_back(get_string(item));
    return paths;
}

std::vector<std::string> string_array_field(const ValuePtr& value, const std::string& key) {
    std::vector<std::string> out;
    auto node = get_field(value, key);
    if (!node || node->kind() != ValueKind::Array) return out;
    for (const auto& item : node->as_array()) out.push_back(get_string(item));
    return out;
}

SourceSpan span_from_value(const ValuePtr& value) {
    if (!value || value->kind() != ValueKind::Object) return {};
    return {
        static_cast<int>(get_field(value, "start_line") && get_field(value, "start_line")->kind() == ValueKind::Integer ? get_field(value, "start_line")->as_int() : 0),
        static_cast<int>(get_field(value, "start_col") && get_field(value, "start_col")->kind() == ValueKind::Integer ? get_field(value, "start_col")->as_int() : 0),
        static_cast<int>(get_field(value, "end_line") && get_field(value, "end_line")->kind() == ValueKind::Integer ? get_field(value, "end_line")->as_int() : 0),
        static_cast<int>(get_field(value, "end_col") && get_field(value, "end_col")->kind() == ValueKind::Integer ? get_field(value, "end_col")->as_int() : 0),
    };
}

ValuePtr unwrap_envelope_value(const ValuePtr& value) {
    if (!value || value->kind() != ValueKind::Object) return value;
    ValuePtr body = get_field(value, "body");
    if (body && (get_field(value, "schemaVersion") || get_field(value, "header") || get_field(value, "envelope"))) {
        return body;
    }
    return value;
}

void collect_templates_from_record(const ValuePtr& raw, std::vector<BindingTemplate>& out) {
    ValuePtr record = unwrap_envelope_value(raw);
    if (!record || record->kind() != ValueKind::Object) return;
    if (get_string(get_field(record, "kind")) != "library-sugar-binding-entry") return;
    std::string target_language = get_string(get_field(record, "target_language"));
    if (!target_language.empty() && target_language != "cpp" && target_language != "cpp-source") return;

    ValuePtr body_source = get_field(record, "body_source");
    if (!body_source || body_source->kind() != ValueKind::Object) return;
    ValuePtr ast_template = get_field(body_source, "ast_template");
    if (!ast_template) ast_template = get_field(body_source, "tree");
    if (!ast_template) return;

    BindingTemplate binding;
    binding.concept_name = get_string(get_field(record, "concept_name"));
    binding.library_tag = get_string(get_field(record, "target_library_tag"), get_string(get_field(record, "library_tag")));
    binding.family = get_field(record, "family") ? get_field(record, "family") : nullv();
    binding.ast_template = ast_template;
    binding.template_cid = get_string(get_field(body_source, "template_cid"));
    if (binding.template_cid.empty()) binding.template_cid = cid_of_value(ast_template);
    binding.param_names = string_array_field(body_source, "param_names");
    if (binding.param_names.empty()) binding.param_names = string_array_field(record, "param_names");
    binding.contract_cid = get_string(get_field(record, "contract_cid"));
    binding.source_function_name = get_string(get_field(record, "source_function_name"));
    if (!binding.template_cid.empty()) out.push_back(std::move(binding));
}

void collect_templates_from_value(const ValuePtr& root, std::vector<BindingTemplate>& out) {
    if (!root) return;
    collect_templates_from_record(root, out);
    ValuePtr members = get_field(root, "members");
    if (members && members->kind() == ValueKind::Array) {
        for (const auto& item : members->as_array()) collect_templates_from_record(item, out);
    } else if (members && members->kind() == ValueKind::Object) {
        for (const auto& [_, item] : members->as_object()) collect_templates_from_record(item, out);
    }
    ValuePtr ir = get_field(root, "ir");
    if (ir && ir->kind() == ValueKind::Array) {
        for (const auto& item : ir->as_array()) collect_templates_from_record(item, out);
    }
}

struct CborNode {
    enum class Kind { Uint, Tstr, Bstr, Array, Map } kind = Kind::Uint;
    uint64_t uint_value = 0;
    std::string text;
    std::vector<uint8_t> bytes;
    std::vector<CborNode> array;
    std::map<std::string, CborNode> map;
};

class CborReader {
   public:
    explicit CborReader(const std::string& bytes)
        : data_(reinterpret_cast<const uint8_t*>(bytes.data())), size_(bytes.size()) {}

    CborNode read() { return read_value(); }

   private:
    void read_head(uint8_t& major, uint64_t& arg) {
        if (pos_ >= size_) throw std::runtime_error("CBOR decode: unexpected EOF");
        uint8_t first = data_[pos_++];
        major = first >> 5;
        uint8_t info = first & 0x1f;
        if (info < 24) {
            arg = info;
        } else if (info == 24) {
            if (pos_ + 1 > size_) throw std::runtime_error("CBOR decode: truncated u8");
            arg = data_[pos_++];
        } else if (info == 25) {
            if (pos_ + 2 > size_) throw std::runtime_error("CBOR decode: truncated u16");
            arg = (uint64_t(data_[pos_]) << 8) | uint64_t(data_[pos_ + 1]);
            pos_ += 2;
        } else if (info == 26) {
            if (pos_ + 4 > size_) throw std::runtime_error("CBOR decode: truncated u32");
            arg = (uint64_t(data_[pos_]) << 24) | (uint64_t(data_[pos_ + 1]) << 16) |
                  (uint64_t(data_[pos_ + 2]) << 8) | uint64_t(data_[pos_ + 3]);
            pos_ += 4;
        } else if (info == 27) {
            if (pos_ + 8 > size_) throw std::runtime_error("CBOR decode: truncated u64");
            arg = 0;
            for (int i = 0; i < 8; ++i) arg = (arg << 8) | uint64_t(data_[pos_ + i]);
            pos_ += 8;
        } else {
            throw std::runtime_error("CBOR decode: indefinite-length item not supported");
        }
    }

    CborNode read_value() {
        uint8_t major = 0;
        uint64_t arg = 0;
        read_head(major, arg);
        CborNode node;
        if (major == 0) {
            node.kind = CborNode::Kind::Uint;
            node.uint_value = arg;
            return node;
        }
        if (major == 2 || major == 3) {
            if (pos_ + arg > size_) throw std::runtime_error("CBOR decode: byte/text string exceeds remaining");
            if (major == 2) {
                node.kind = CborNode::Kind::Bstr;
                node.bytes.assign(data_ + pos_, data_ + pos_ + arg);
            } else {
                node.kind = CborNode::Kind::Tstr;
                node.text.assign(reinterpret_cast<const char*>(data_ + pos_), static_cast<size_t>(arg));
            }
            pos_ += static_cast<size_t>(arg);
            return node;
        }
        if (major == 4) {
            node.kind = CborNode::Kind::Array;
            for (uint64_t i = 0; i < arg; ++i) node.array.push_back(read_value());
            return node;
        }
        if (major == 5) {
            node.kind = CborNode::Kind::Map;
            for (uint64_t i = 0; i < arg; ++i) {
                CborNode key = read_value();
                if (key.kind != CborNode::Kind::Tstr) throw std::runtime_error("CBOR decode: map key is not text");
                node.map.emplace(key.text, read_value());
            }
            return node;
        }
        throw std::runtime_error("CBOR decode: unsupported major type " + std::to_string(major));
    }

    const uint8_t* data_;
    size_t size_ = 0;
    size_t pos_ = 0;
};

void collect_templates_from_member_bytes(const std::vector<uint8_t>& bytes, std::vector<BindingTemplate>& out) {
    std::string text(reinterpret_cast<const char*>(bytes.data()), bytes.size());
    collect_templates_from_value(JsonParser(text).parse(), out);
}

void collect_templates_from_cbor_catalog(const std::string& bytes, std::vector<BindingTemplate>& out) {
    CborNode root = CborReader(bytes).read();
    if (root.kind != CborNode::Kind::Map) return;
    auto it = root.map.find("members");
    if (it == root.map.end()) return;
    const CborNode& members = it->second;
    if (members.kind == CborNode::Kind::Map) {
        for (const auto& [_, member] : members.map) {
            if (member.kind == CborNode::Kind::Bstr) collect_templates_from_member_bytes(member.bytes, out);
        }
    } else if (members.kind == CborNode::Kind::Array) {
        for (const auto& member : members.array) {
            if (member.kind == CborNode::Kind::Bstr) collect_templates_from_member_bytes(member.bytes, out);
        }
    }
}

std::string read_binary_file(const std::filesystem::path& path) {
    std::ifstream in(path, std::ios::binary);
    if (!in) throw std::runtime_error("cannot open " + path.string());
    std::ostringstream buf;
    buf << in.rdbuf();
    return buf.str();
}

void append_proofs_in_dir(const std::filesystem::path& dir, bool recursive, std::vector<std::filesystem::path>& out) {
    std::error_code ec;
    if (!std::filesystem::exists(dir, ec) || !std::filesystem::is_directory(dir, ec)) return;
    if (recursive) {
        for (const auto& entry : std::filesystem::recursive_directory_iterator(dir, ec)) {
            if (ec) break;
            if (entry.is_regular_file(ec) && entry.path().extension() == ".proof") out.push_back(entry.path());
        }
    } else {
        for (const auto& entry : std::filesystem::directory_iterator(dir, ec)) {
            if (ec) break;
            if (entry.is_regular_file(ec) && entry.path().extension() == ".proof") out.push_back(entry.path());
        }
    }
}

std::vector<std::filesystem::path> enumerate_recognizer_proofs(const std::string& project_root) {
    std::vector<std::filesystem::path> out;
    std::filesystem::path root(project_root.empty() ? "." : project_root);
    append_proofs_in_dir(root, false, out);
    append_proofs_in_dir(root / ".provekit", true, out);
    std::error_code ec;
    if (std::filesystem::exists(root, ec) && std::filesystem::is_directory(root, ec)) {
        for (const auto& entry : std::filesystem::directory_iterator(root, ec)) {
            if (ec) break;
            if (entry.is_directory(ec) && entry.path().filename() != ".git" && entry.path().filename() != ".provekit") {
                append_proofs_in_dir(entry.path(), false, out);
            }
        }
    }
    return out;
}

std::vector<BindingTemplate> load_binding_templates_from_project(const std::string& project_root, std::vector<ValuePtr>& diagnostics) {
    std::vector<BindingTemplate> bindings;
    for (const auto& proof_path : enumerate_recognizer_proofs(project_root)) {
        try {
            std::string bytes = read_binary_file(proof_path);
            size_t first = bytes.find_first_not_of(" \t\r\n");
            if (first != std::string::npos && (bytes[first] == '{' || bytes[first] == '[')) {
                collect_templates_from_value(JsonParser(bytes).parse(), bindings);
            } else {
                collect_templates_from_cbor_catalog(bytes, bindings);
            }
        } catch (const std::exception& ex) {
            diagnostics.push_back(obj({
                {"message", strv("recognize proof load skipped " + proof_path.string() + ": " + ex.what())},
                {"severity", strv("warning")},
            }));
        }
    }
    return bindings;
}

ValuePtr family_or_null(const ValuePtr& family) { return family ? family : nullv(); }

ValuePtr recognize_tag_value(const RecognizeTag& tag) {
    std::vector<ValuePtr> param_bindings;
    for (const auto& binding : tag.param_bindings) {
        param_bindings.push_back(obj({{"index", intv(binding.index)}, {"source_text", strv(binding.source_text)}}));
    }
    return obj({
        {"concept_name", strv(tag.concept_name)},
        {"contract_cid", tag.contract_cid.empty() ? nullv() : strv(tag.contract_cid)},
        {"family", family_or_null(tag.family)},
        {"file", strv(tag.file)},
        {"function_name", strv(tag.function_name)},
        {"library_tag", strv(tag.library_tag)},
        {"match_tier", strv(tag.match_tier)},
        {"param_bindings", arr(param_bindings)},
        {"span", span_value(tag.span)},
        {"template_cid", strv(tag.template_cid)},
    });
}

ValuePtr recognize_result_value(const RecognizeResult& result) {
    std::vector<ValuePtr> tags;
    for (const auto& tag : result.tags) tags.push_back(recognize_tag_value(tag));
    std::vector<ValuePtr> refusals;
    for (const auto& r : result.refusals) {
        std::vector<std::pair<std::string, ValuePtr>> fields{{"kind", strv(r.kind)}, {"reason", strv(r.reason)}};
        if (!r.function.empty()) fields.push_back({"function", strv(r.function)});
        if (r.line > 0) fields.push_back({"line", intv(r.line)});
        refusals.push_back(obj(fields));
    }
    return obj({{"diagnostics", arr(result.diagnostics)}, {"refusals", arr(refusals)}, {"tags", arr(tags)}});
}

}  // namespace

std::string recognize_result_json(const RecognizeResult& result) {
    return canonical_bytes(recognize_result_value(result));
}

RecognizeResult recognize_source(const std::string& path, const std::string& source, const std::vector<BindingTemplate>& bindings) {
    RecognizeResult result;
    std::map<std::string, BindingTemplate> bindings_by_cid;
    for (const auto& binding : bindings) {
        if (!binding.template_cid.empty()) bindings_by_cid[binding.template_cid] = binding;
    }

    LiftResult lifted = lift_source(path, source);
    result.diagnostics.insert(result.diagnostics.end(), lifted.diagnostics.begin(), lifted.diagnostics.end());
    result.refusals.insert(result.refusals.end(), lifted.refusals.begin(), lifted.refusals.end());

    for (const auto& decl : lifted.declarations) {
        if (get_string(get_field(decl, "kind")) != "function-contract") continue;
        std::string function_name = get_string(get_field(decl, "fnName"));
        if (function_name.find("<source-unit:") != std::string::npos) continue;
        ValuePtr body_source = get_field(decl, "body_source");
        if (!body_source) continue;
        std::string template_cid = get_string(get_field(body_source, "template_cid"));
        auto binding_it = bindings_by_cid.find(template_cid);
        if (binding_it == bindings_by_cid.end()) continue;
        const BindingTemplate& binding = binding_it->second;

        std::vector<ParamBinding> param_bindings;
        std::vector<std::string> param_names = string_array_field(body_source, "param_names");
        for (size_t i = 0; i < param_names.size(); ++i) {
            param_bindings.push_back({static_cast<int>(i + 1), param_names[i]});
        }

        result.tags.push_back({
            path,
            span_from_value(get_field(body_source, "span")),
            function_name,
            binding.concept_name,
            binding.library_tag,
            family_or_null(binding.family),
            template_cid,
            binding.contract_cid,
            "exact",
            param_bindings,
        });
    }
    return result;
}

RecognizeResult recognize_paths(const std::string& project_root, const std::vector<std::string>& source_paths) {
    RecognizeResult aggregate;
    std::vector<ValuePtr> diagnostics;
    std::vector<BindingTemplate> bindings = load_binding_templates_from_project(project_root, diagnostics);
    aggregate.diagnostics.insert(aggregate.diagnostics.end(), diagnostics.begin(), diagnostics.end());

    for (const auto& rel : source_paths) {
        std::filesystem::path path(rel);
        if (!path.is_absolute()) path = std::filesystem::path(project_root.empty() ? "." : project_root) / rel;
        std::ifstream in(path, std::ios::binary);
        if (!in) {
            aggregate.diagnostics.push_back(obj({{"message", strv("recognize source path not found: " + path.string())}, {"severity", strv("warning")}}));
            continue;
        }
        std::ostringstream buf;
        buf << in.rdbuf();
        RecognizeResult one = recognize_source(rel, buf.str(), bindings);
        aggregate.tags.insert(aggregate.tags.end(), one.tags.begin(), one.tags.end());
        aggregate.refusals.insert(aggregate.refusals.end(), one.refusals.begin(), one.refusals.end());
        aggregate.diagnostics.insert(aggregate.diagnostics.end(), one.diagnostics.begin(), one.diagnostics.end());
    }
    return aggregate;
}

int run_rpc() {
    std::string line;
    while (std::getline(std::cin, line)) {
        if (line.empty()) continue;
        try {
            ValuePtr req = JsonParser(line).parse();
            ValuePtr id = request_id(req);
            std::string method = get_string(get_field(req, "method"));
            if (method == "initialize") {
                std::cout << response(id, initialize_result()) << "\n";
            } else if (method == "lift") {
                ValuePtr params = get_field(req, "params");
                std::string surface = get_string(get_field(params, "surface"), DIALECT);
                if (surface != DIALECT) {
                    std::cout << error_response(id, 1003, "SURFACE_NOT_SUPPORTED: " + surface) << "\n";
                    continue;
                }
                std::vector<std::string> paths = source_paths_from_params(params);
                if (paths.empty()) {
                    std::cout << error_response(id, -32602, "source_paths must be a non-empty array of strings") << "\n";
                    continue;
                }
                std::string root = get_string(get_field(params, "workspace_root"), ".");
                LiftResult lifted = lift_paths(root, paths);
                std::cout << response(id, JsonParser(lift_result_json(lifted)).parse()) << "\n";
            } else if (method == "compile") {
                ValuePtr params = get_field(req, "params");
                ValuePtr ir = get_field(params, "ir");
                if (!ir || ir->kind() != ValueKind::Array) {
                    std::cout << error_response(id, -32602, "ir must be an array") << "\n";
                    continue;
                }
                std::string body = compile_ir_document(ir->as_array());
                std::cout << response(id, obj({{"body", strv(body)}, {"kind", strv("compiled-formula")}})) << "\n";
            } else if (method == "provekit.plugin.recognize" || method == "recognize") {
                ValuePtr params = get_field(req, "params");
                std::vector<std::string> paths = source_paths_from_params(params);
                if (paths.empty()) {
                    std::cout << error_response(id, -32602, "source_paths must be a non-empty array of strings") << "\n";
                    continue;
                }
                std::string root = get_string(get_field(params, "project_root"));
                if (root.empty()) root = get_string(get_field(params, "workspace_root"), ".");
                RecognizeResult recognized = recognize_paths(root, paths);
                std::cout << response(id, recognize_result_value(recognized)) << "\n";
            } else if (method == "shutdown") {
                std::cout << response(id, nullptr) << "\n";
                return 0;
            } else {
                std::cout << error_response(id, -32601, "METHOD_NOT_FOUND: " + method) << "\n";
            }
        } catch (const std::exception& ex) {
            std::cout << error_response(nullv(), -32603, ex.what()) << "\n";
        }
        std::cout.flush();
    }
    return 0;
}

}  // namespace provekit::cpp_source
