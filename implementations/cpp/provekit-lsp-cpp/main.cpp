// SPDX-License-Identifier: Apache-2.0
//
// provekit-lsp-cpp: canonical NDJSON LSP plugin for C++.
//
// Protocol (provekit-lift/1 over stdio):
//
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//   {"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":"...","source_paths":[...]}}
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
//
// Legacy parse method is retained for backward compatibility.
//
// Wire shape matches implementations/go/provekit-lift-go/rpc.go.
//
// Binary name: provekit-lsp-cpp (no args required; reads NDJSON from stdin)

#include "provekit/ir.hpp"

#include <fstream>
#include <iostream>
#include <regex>
#include <sstream>
#include <string>
#include <vector>

using namespace provekit::ir;

// ---------------------------------------------------------------------------
// Annotation scanning (replicates provekit-lift-cpp/main.cpp scan_file)
// ---------------------------------------------------------------------------

struct Annotation {
    std::string function_name;
    enum Kind { Contract, Implement, Verify } kind;
    std::string target_cid;
    int line;
};

static std::string trim(const std::string& s) {
    size_t start = s.find_first_not_of(" \t");
    if (start == std::string::npos) return "";
    size_t end = s.find_last_not_of(" \t");
    return s.substr(start, end - start + 1);
}

static std::string find_ahead_fn(const std::vector<std::string>& lines, int start_line) {
    int max_line = static_cast<int>(lines.size());
    if (start_line + 10 < max_line) max_line = start_line + 10;
    std::regex fn_re(R"(^\s*(?:(?:auto|void|int|bool|float|double|std::string|size_t)\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*\()");
    for (int i = start_line + 1; i < max_line; i++) {
        std::smatch m;
        if (std::regex_search(lines[i], m, fn_re)) {
            return m[1].str();
        }
    }
    return "unknown";
}

static std::vector<Annotation> scan_source(const std::string& text) {
    std::vector<Annotation> anns;
    std::vector<std::string> lines;
    std::istringstream iss(text);
    std::string line;
    while (std::getline(iss, line)) lines.push_back(line);

    for (size_t i = 0; i < lines.size(); i++) {
        std::string trimmed = trim(lines[i]);
        if (trimmed.find("//provekit:contract") == 0) {
            anns.push_back({find_ahead_fn(lines, (int)i), Annotation::Contract, "", (int)i});
        } else if (trimmed.find("//provekit:implement") == 0) {
            std::string cid = trim(trimmed.substr(20));
            anns.push_back({find_ahead_fn(lines, (int)i), Annotation::Implement, cid, (int)i});
        } else if (trimmed.find("//provekit:verify") == 0) {
            anns.push_back({find_ahead_fn(lines, (int)i), Annotation::Verify, "", (int)i});
        }
    }
    return anns;
}

// ---------------------------------------------------------------------------
// Lift annotations -> IR declarations via provekit/ir.hpp
// ---------------------------------------------------------------------------

static std::string lift_to_declarations_json(const std::string& source) {
    reset_collector();
    begin_collecting();

    auto anns = scan_source(source);
    for (const auto& ann : anns) {
        if (ann.kind == Annotation::Contract) {
            // Emit contract with placeholder true postcondition.
            auto post = atomic_("true", {});
            contract(ann.function_name, nullptr, post);
        }
        // Implement and Verify annotations: not emitted as contract declarations.
        // Bridge emission requires full mint pipeline; out of scope for LSP parse.
    }

    auto decls = finish();
    return marshal_declarations(decls);
}

// ---------------------------------------------------------------------------
// Minimal JSON helpers (no external dep)
// ---------------------------------------------------------------------------

// Extract string value for a given key from a flat JSON line.
// Handles: "key":"value" (with basic escape pass-through).
static std::string extract_string(const std::string& line, const std::string& key) {
    std::string search = "\"" + key + "\"";
    size_t pos = line.find(search);
    if (pos == std::string::npos) return "";
    size_t colon = line.find(':', pos + search.size());
    if (colon == std::string::npos) return "";
    // Skip whitespace
    size_t vstart = line.find_first_not_of(" \t", colon + 1);
    if (vstart == std::string::npos) return "";
    if (line[vstart] != '"') return "";
    // Scan for closing quote, respecting backslash escapes
    std::string result;
    for (size_t j = vstart + 1; j < line.size(); j++) {
        if (line[j] == '\\' && j + 1 < line.size()) {
            // Pass escape through (we only need rough string extraction here)
            result += line[j];
            result += line[j + 1];
            j++;
        } else if (line[j] == '"') {
            break;
        } else {
            result += line[j];
        }
    }
    return result;
}

// Unescape a JSON string value (basic: handles \n \t \r \\ \").
static std::string unescape_json(const std::string& s) {
    std::string out;
    for (size_t i = 0; i < s.size(); i++) {
        if (s[i] == '\\' && i + 1 < s.size()) {
            switch (s[i + 1]) {
                case '"':  out += '"';  i++; break;
                case '\\': out += '\\'; i++; break;
                case 'n':  out += '\n'; i++; break;
                case 'r':  out += '\r'; i++; break;
                case 't':  out += '\t'; i++; break;
                default:   out += s[i]; break;
            }
        } else {
            out += s[i];
        }
    }
    return out;
}

// Extract the method name from a JSON-RPC line.
static std::string extract_method(const std::string& line) {
    return extract_string(line, "method");
}

// Extract the id field. Returns raw JSON token (number or "null").
static std::string extract_id(const std::string& line) {
    std::regex id_re(R"("id"\s*:\s*(\d+|null))");
    std::smatch m;
    if (std::regex_search(line, m, id_re)) return m[1].str();
    return "null";
}

// Extract a JSON string array field from a JSON line.
// Returns a vector of unescaped strings.
static std::vector<std::string> extract_string_array(const std::string& line, const std::string& key) {
    std::string search = "\"" + key + "\"";
    size_t pos = line.find(search);
    if (pos == std::string::npos) return {};
    size_t colon = line.find(':', pos + search.size());
    if (colon == std::string::npos) return {};
    size_t bracket = line.find('[', colon + 1);
    if (bracket == std::string::npos) return {};

    std::vector<std::string> result;
    size_t i = bracket + 1;
    while (i < line.size()) {
        while (i < line.size() && (line[i] == ' ' || line[i] == '\t' || line[i] == ',')) ++i;
        if (i >= line.size() || line[i] == ']') break;
        if (line[i] != '"') break;
        ++i;
        std::string elem;
        while (i < line.size() && line[i] != '"') {
            if (line[i] == '\\' && i + 1 < line.size()) {
                ++i;
                switch (line[i]) {
                    case '"':  elem += '"';  break;
                    case '\\': elem += '\\'; break;
                    case 'n':  elem += '\n'; break;
                    case 'r':  elem += '\r'; break;
                    case 't':  elem += '\t'; break;
                    default:   elem += line[i]; break;
                }
            } else {
                elem += line[i];
            }
            ++i;
        }
        if (i < line.size() && line[i] == '"') ++i;
        result.push_back(std::move(elem));
    }
    return result;
}

// Read entire file to string. Returns empty string on failure.
static std::string read_file(const std::string& path) {
    std::ifstream f(path);
    if (!f.is_open()) return "";
    return std::string(std::istreambuf_iterator<char>(f), std::istreambuf_iterator<char>());
}

// ---------------------------------------------------------------------------
// Response writers
// ---------------------------------------------------------------------------

static void send_result(const std::string& id, const std::string& result_json) {
    std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id
              << ",\"result\":" << result_json << "}\n"
              << std::flush;
}

static void send_error(const std::string& id, int code, const std::string& msg) {
    std::string safe_msg;
    for (char c : msg) {
        switch (c) {
            case '"':  safe_msg += "\\\""; break;
            case '\\': safe_msg += "\\\\"; break;
            case '\n': safe_msg += "\\n";  break;
            case '\r': safe_msg += "\\r";  break;
            case '\t': safe_msg += "\\t";  break;
            default:   safe_msg += c;      break;
        }
    }
    std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id
              << ",\"error\":{\"code\":" << code
              << ",\"message\":\"" << safe_msg << "\"}}\n"
              << std::flush;
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

int main() {
    std::string line;
    while (std::getline(std::cin, line)) {
        if (line.empty()) continue;

        std::string id = extract_id(line);
        std::string method = extract_method(line);

        if (method == "initialize") {
            send_result(id,
                "{\"capabilities\":{"
                "\"authoring_surfaces\":[\"cpp-source\"],"
                "\"emits_signed_mementos\":false,"
                "\"ir_version\":\"v1.1.0\"},"
                "\"name\":\"provekit-lsp-cpp\","
                "\"protocol_version\":\"provekit-lift/1\","
                "\"version\":\"0.1.0\"}");

        } else if (method == "lift") {
            // Extract workspace_root and source_paths from params.
            std::string workspace_root = extract_string(line, "workspace_root");
            if (workspace_root.empty()) workspace_root = ".";

            std::vector<std::string> source_paths = extract_string_array(line, "source_paths");
            if (source_paths.empty()) {
                send_error(id, -32602, "lift: source_paths must be a non-empty array");
                continue;
            }

            // Lift each file; aggregate declarations.
            reset_collector();
            begin_collecting();

            for (const auto& sp : source_paths) {
                std::string full_path = sp;
                if (!sp.empty() && sp[0] != '/') {
                    full_path = workspace_root + "/" + sp;
                }
                std::string source = read_file(full_path);
                if (source.empty()) continue;

                auto anns = scan_source(source);
                for (const auto& ann : anns) {
                    if (ann.kind == Annotation::Contract) {
                        auto post = atomic_("true", {});
                        contract(ann.function_name, nullptr, post);
                    }
                }
            }

            auto decls = finish();
            std::string ir_json = marshal_declarations(decls);
            send_result(id,
                "{\"callEdges\":[],"
                "\"diagnostics\":[],"
                "\"ir\":" + ir_json + ","
                "\"kind\":\"ir-document\","
                "\"opacityReport\":[],"
                "\"refusals\":[]}");

        } else if (method == "parse") {
            // Extract path and source from params.
            // source may be a multi-line JSON string; extract_string handles
            // the escaped version we receive from the caller.
            std::string source_escaped = extract_string(line, "source");
            std::string source = unescape_json(source_escaped);

            std::string decls_json = lift_to_declarations_json(source);

            // callEdges: always empty (no call-graph analysis in cpp LSP).
            // warnings: always empty.
            std::string result = "{\"callEdges\":[],\"declarations\":" + decls_json + ",\"warnings\":[]}";
            send_result(id, result);

        } else if (method == "shutdown") {
            send_result(id, "null");
            return 0;

        } else {
            send_error(id, -32601, "unknown method: " + method);
        }
    }
    return 0;
}
