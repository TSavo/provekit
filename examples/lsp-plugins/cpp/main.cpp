// ProvekIt LSP Language Plugin — C++
//
// A standalone binary that speaks provekit-lsp-plugin/1 over stdio.
// Parses C++ source files and extracts provekit annotations.
//
// Usage: ./provekit-lsp-cpp --rpc
//
// To use this plugin, add to `.provekit/config.toml`:
//   [[language]]
//   name = "cpp"
//   extensions = [".cpp", ".cc", ".h", ".hpp"]
//   plugin = "provekit-lsp-cpp"
//
// Build: g++ -std=c++17 -o provekit-lsp-cpp main.cpp

#include <iostream>
#include <string>
#include <vector>
#include <regex>
#include <optional>
#include <sstream>

struct Position {
    uint32_t line;
    uint32_t character;
};

struct Range {
    Position start;
    Position end;
};

struct Annotation {
    std::string function_name;
    std::string kind;
    std::optional<std::string> target_cid;
    Range range;
};

std::string escape_json(const std::string& s) {
    std::string out;
    for (char c : s) {
        switch (c) {
            case '"': out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\b': out += "\\b"; break;
            case '\f': out += "\\f"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default: out += c; break;
        }
    }
    return out;
}

std::string annotation_to_json(const Annotation& a) {
    std::string json = "{";
    json += "\"function_name\":\"" + escape_json(a.function_name) + "\",";
    json += "\"kind\":\"" + escape_json(a.kind) + "\",";
    if (a.target_cid.has_value()) {
        json += "\"target_cid\":\"" + escape_json(a.target_cid.value()) + "\",";
    }
    json += "\"range\":{";
    json += "\"start\":{\"line\":" + std::to_string(a.range.start.line) + ",\"character\":" + std::to_string(a.range.start.character) + "},";
    json += "\"end\":{\"line\":" + std::to_string(a.range.end.line) + ",\"character\":" + std::to_string(a.range.end.character) + "}";
    json += "}}";
    return json;
}

std::vector<std::string> split_lines(const std::string& text) {
    std::vector<std::string> lines;
    size_t start = 0;
    size_t end = text.find('\n');
    while (end != std::string::npos) {
        lines.push_back(text.substr(start, end - start));
        start = end + 1;
        end = text.find('\n', start);
    }
    lines.push_back(text.substr(start));
    return lines;
}

std::string find_ahead(const std::vector<std::string>& lines, size_t start, const std::regex& re) {
    std::smatch match;
    for (size_t j = start + 1; j < lines.size() && j < start + 10; ++j) {
        if (std::regex_search(lines[j], match, re)) {
            return match[1].str();
        }
    }
    return "unknown";
}

std::vector<Annotation> parse_cpp(const std::string& text) {
    std::vector<Annotation> annotations;
    auto lines = split_lines(text);

    std::regex re_impl(R"(//\s*provekit:implement\s+([\w-]+))");
    std::regex re_contract(R"(//\s*provekit:contract)");
    std::regex re_verify(R"(//\s*provekit:verify)");
    std::regex re_fn(R"(\b(?:void|int|auto|bool|string|double|float|\w+)\s+(\w+)\s*\()");

    for (size_t i = 0; i < lines.size(); ++i) {
        const auto& line = lines[i];
        std::smatch match;

        if (std::regex_search(line, match, re_impl)) {
            std::string cid = match[1].str();
            std::string fn_name = find_ahead(lines, i, re_fn);
            Annotation a;
            a.function_name = fn_name;
            a.kind = "implement";
            a.target_cid = cid;
            a.range = {{(uint32_t)i, 0}, {(uint32_t)(i + 1), 0}};
            annotations.push_back(a);
        }

        if (std::regex_search(line, match, re_contract)) {
            std::string fn_name = find_ahead(lines, i, re_fn);
            Annotation a;
            a.function_name = fn_name;
            a.kind = "contract";
            a.target_cid = std::nullopt;
            a.range = {{(uint32_t)i, 0}, {(uint32_t)(i + 1), 0}};
            annotations.push_back(a);
        }

        if (std::regex_search(line, match, re_verify)) {
            std::string fn_name = find_ahead(lines, i, re_fn);
            Annotation a;
            a.function_name = fn_name;
            a.kind = "verify";
            a.target_cid = std::nullopt;
            a.range = {{(uint32_t)i, 0}, {(uint32_t)(i + 1), 0}};
            annotations.push_back(a);
        }
    }

    return annotations;
}

int main(int argc, char** argv) {
    bool rpc_mode = false;
    for (int i = 1; i < argc; ++i) {
        if (std::string(argv[i]) == "--rpc") {
            rpc_mode = true;
            break;
        }
    }

    if (!rpc_mode) {
        std::cerr << "Usage: provekit-lsp-cpp --rpc" << std::endl;
        return 1;
    }

    std::string line;
    while (std::getline(std::cin, line)) {
        std::string method;
        std::string id = "null";

        size_t method_pos = line.find("\"method\"");
        if (method_pos != std::string::npos) {
            size_t quote = line.find('"', method_pos + 10);
            if (quote != std::string::npos) {
                size_t end_quote = line.find('"', quote + 1);
                method = line.substr(quote + 1, end_quote - quote - 1);
            }
        }

        size_t id_pos = line.find("\"id\"");
        if (id_pos != std::string::npos) {
            size_t colon = line.find(':', id_pos + 4);
            if (colon != std::string::npos) {
                size_t comma = line.find(',', colon + 1);
                size_t end = (comma != std::string::npos) ? comma : line.find('}', colon + 1);
                id = line.substr(colon + 1, end - colon - 1);
                size_t start = id.find_first_not_of(" \t");
                if (start != std::string::npos) id = id.substr(start);
            }
        }

        if (method == "initialize") {
            std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id << ",\"result\":{\"name\":\"provekit-lsp-cpp\",\"version\":\"0.1.0\",\"capabilities\":[]}}" << std::endl;
        } else if (method == "parse") {
            size_t text_pos = line.find("\"text\"");
            std::string text;
            if (text_pos != std::string::npos) {
                size_t quote = line.find('"', text_pos + 8);
                if (quote != std::string::npos) {
                    size_t end_quote = line.find('"', quote + 1);
                    text = line.substr(quote + 1, end_quote - quote - 1);
                }
            }
            auto annotations = parse_cpp(text);
            std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id << ",\"result\":{\"annotations\":[";
            for (size_t i = 0; i < annotations.size(); ++i) {
                if (i > 0) std::cout << ",";
                std::cout << annotation_to_json(annotations[i]);
            }
            std::cout << "]}}" << std::endl;
        } else if (method == "shutdown") {
            std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id << ",\"result\":null}" << std::endl;
            return 0;
        } else {
            std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id << ",\"error\":{\"code\":-32601,\"message\":\"unknown method: " << method << "\"}}" << std::endl;
        }
    }

    return 0;
}
