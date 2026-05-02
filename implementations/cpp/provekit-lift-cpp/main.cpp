// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-cpp — lifts C++ contracts to canonical IR.
//
// Scans C++ source for:
//   1. //provekit:contract               → contract declaration
//   2. //provekit:implement <cid>        → bridge to external contract
//   3. //provekit:verify                 → verification check
//   4. [[expects: condition]]            → precondition (C++26 contracts)
//   5. [[ensures:  condition]]           → postcondition (C++26 contracts)
//
// Usage:
//   provekit-lift-cpp --workspace ./src --out ./target/provekit
//   provekit-lift-cpp --rpc              (NDJSON JSON-RPC plugin mode)
//
// Emits JCS canonical IR via provekit::ir::marshal_declarations.

#include "provekit/ir.hpp"

#include <cstdio>
#include <fstream>
#include <functional>
#include <iostream>
#include <regex>
#include <string>
#include <vector>

using namespace provekit::ir;

#ifdef _WIN32
#include <windows.h>
#else
#include <dirent.h>
#include <sys/stat.h>
#include <sys/types.h>
#endif

namespace {

// -----------------------------------------------------------------------
// Annotation scanning
// -----------------------------------------------------------------------

struct Annotation {
    std::string function_name;
    enum Kind { Contract, Implement, Verify } kind;
    std::string target_cid;   // for Implement
    std::string expects_expr; // for C++26 contracts
    std::string ensures_expr;
    int line;
};

std::string trim(const std::string& s) {
    size_t start = s.find_first_not_of(" \t");
    if (start == std::string::npos) return "";
    size_t end = s.find_last_not_of(" \t");
    return s.substr(start, end - start + 1);
}

std::string find_ahead_fn(const std::vector<std::string>& lines, int start_line) {
    int max_line = std::min<int>(lines.size(), start_line + 10);
    for (int i = start_line + 1; i < max_line; i++) {
        const std::string& l = lines[i];
        // Match: return_type function_name(...)
        // or: auto function_name(...)
        // or: void function_name(...)
        std::regex fn_re(R"(^\s*(?:(?:auto|void|int|bool|float|double|std::string|size_t)\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*\()");
        std::smatch m;
        if (std::regex_search(l, m, fn_re)) {
            return m[1].str();
        }
    }
    return "unknown";
}

std::vector<Annotation> scan_file(const std::string& text) {
    std::vector<Annotation> anns;
    std::vector<std::string> lines;
    std::istringstream iss(text);
    std::string line;
    while (std::getline(iss, line)) lines.push_back(line);

    for (size_t i = 0; i < lines.size(); i++) {
        const std::string& l = lines[i];
        std::string trimmed = trim(l);

        if (trimmed.find("//provekit:contract") == 0) {
            anns.push_back({find_ahead_fn(lines, i), Annotation::Contract, "", "", "", (int)i});
        } else if (trimmed.find("//provekit:implement") == 0) {
            std::string cid = trim(trimmed.substr(22));
            anns.push_back({find_ahead_fn(lines, i), Annotation::Implement, cid, "", "", (int)i});
        } else if (trimmed.find("//provekit:verify") == 0) {
            anns.push_back({find_ahead_fn(lines, i), Annotation::Verify, "", "", "", (int)i});
        } else if (trimmed.find("[[expects:") == 0 || trimmed.find("[[expects ") == 0) {
            std::regex expects_re(R"(\[\[expects:?\s*(.+?)\]\])");
            std::smatch m;
            if (std::regex_search(l, m, expects_re)) {
                anns.push_back({find_ahead_fn(lines, i), Annotation::Contract,
                               "", m[1].str(), "", (int)i});
            }
        } else if (trimmed.find("[[ensures:") == 0 || trimmed.find("[[ensures ") == 0) {
            std::regex ensures_re(R"(\[\[ensures:?\s*(.+?)\]\])");
            std::smatch m;
            if (std::regex_search(l, m, ensures_re)) {
                anns.push_back({find_ahead_fn(lines, i), Annotation::Contract,
                               "", "", m[1].str(), (int)i});
            }
        }
    }
    return anns;
}

// -----------------------------------------------------------------------
// IR construction from annotation
// -----------------------------------------------------------------------

void lift_annotations(std::vector<ContractDecl>& decls,
                      const std::vector<Annotation>& anns) {
    for (const auto& ann : anns) {
        switch (ann.kind) {
            case Annotation::Contract: {
                // Emit a simple contract with the function name and a
                // placeholder `true` postcondition. Real expression lifting
                // (parsing C++ boolean expressions into IR atoms) is
                // planned for v1.2.
                auto post = std::make_shared<Formula>(Formula{
                    AtomicFormula{"true", {}}});
                contract(ann.function_name, nullptr, post);
                break;
            }
            case Annotation::Implement: {
                // Bridge declaration: source is C++, target is the given CID.
                // TODO: emit as BridgeDecl via gather system.
                // Currently logged but not emitted as contract.
                break;
            }
            case Annotation::Verify: {
                // Mark for verification against its contract.
                // Currently logged but requires a verifier backend.
                break;
            }
        }
    }
}

// -----------------------------------------------------------------------
// File traversal
// -----------------------------------------------------------------------

void walk_files(const std::string& dir_path,
                std::vector<std::string>& files) {
#ifdef _WIN32
    WIN32_FIND_DATAA fd;
    std::string pattern = dir_path + "\\*";
    HANDLE h = FindFirstFileA(pattern.c_str(), &fd);
    if (h == INVALID_HANDLE_VALUE) return;
    do {
        std::string name = fd.cFileName;
        if (name == "." || name == "..") continue;
        std::string full = dir_path + "\\" + name;
        if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) {
            walk_files(full, files);
        } else {
            std::string ext = name.substr(name.find_last_of('.') + 1);
            if (ext == "cpp" || ext == "cc" || ext == "cxx" || ext == "hpp" || ext == "h") {
                files.push_back(full);
            }
        }
    } while (FindNextFileA(h, &fd));
    FindClose(h);
#else
    DIR* dir = opendir(dir_path.c_str());
    if (!dir) return;
    struct dirent* entry;
    while ((entry = readdir(dir)) != nullptr) {
        std::string name = entry->d_name;
        if (name == "." || name == "..") continue;
        std::string full = dir_path + "/" + name;
        struct stat st;
        if (stat(full.c_str(), &st) != 0) continue;
        if (S_ISDIR(st.st_mode)) {
            walk_files(full, files);
        } else {
            std::string ext;
            auto dot_pos = name.find_last_of('.');
            if (dot_pos != std::string::npos) ext = name.substr(dot_pos + 1);
            if (ext == "cpp" || ext == "cc" || ext == "cxx" || ext == "hpp" || ext == "h") {
                files.push_back(full);
            }
        }
    }
    closedir(dir);
#endif
}

std::string read_file(const std::string& path) {
    std::ifstream f(path);
    if (!f) return "";
    std::string content((std::istreambuf_iterator<char>(f)),
                        std::istreambuf_iterator<char>());
    return content;
}

// -----------------------------------------------------------------------
// RPC plugin mode (NDJSON)
// -----------------------------------------------------------------------

std::string extract_id(const std::string& line) {
    std::regex id_re("\"id\":\\s*(\\d+)");
    std::smatch m;
    if (std::regex_search(line, m, id_re)) return m[1].str();
    return "null";
}

void rpc_mode(const std::string& workspace_path) {
    std::string line;
    while (true) {
        if (!std::getline(std::cin, line)) break;

        bool has_init = line.find("\"initialize\"") != std::string::npos;
        bool has_parse = line.find("\"parse\"") != std::string::npos;
        bool has_shutdown = line.find("\"shutdown\"") != std::string::npos;
        std::string id = extract_id(line);

        if (has_init) {
            std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id
                      << ",\"result\":{\"name\":\"provekit-lift-cpp\",\"version\":\"0.1.0\","
                      << "\"capabilities\":[\"parse\"]}}\n" << std::flush;
        } else if (has_parse) {
            // Collect annotations from workspace
            std::vector<std::string> files;
            walk_files(workspace_path, files);

            reset_collector();
            begin_collecting();

            for (const auto& path : files) {
                std::string text = read_file(path);
                if (text.empty()) continue;
                auto anns = scan_file(text);
                std::vector<ContractDecl> decls;
                lift_annotations(decls, anns);
                for (auto& d : decls) {
                    collector().push_back(std::move(d));
                }
            }

            auto all_decls = finish();
            std::string json = marshal_declarations(all_decls);

            std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id
                      << ",\"result\":{\"declarations\":" << json
                      << ",\"warnings\":[]}}\n" << std::flush;
        } else if (has_shutdown) {
            std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id
                      << ",\"result\":null}\n" << std::flush;
            break;
        } else {
            std::cout << "{\"jsonrpc\":\"2.0\",\"id\":" << id
                      << ",\"error\":{\"code\":-32601,\"message\":\"unknown method\"}}\n"
                      << std::flush;
        }
    }
}

// -----------------------------------------------------------------------
// Standalone mode
// -----------------------------------------------------------------------

void standalone_mode(const std::string& workspace_path,
                     const std::string& output_path) {
    std::vector<std::string> files;
    walk_files(workspace_path, files);

    reset_collector();
    begin_collecting();

    for (const auto& path : files) {
        std::string text = read_file(path);
        if (text.empty()) continue;
        auto anns = scan_file(text);
        std::vector<ContractDecl> decls;
        lift_annotations(decls, anns);
        for (auto& d : decls) {
            collector().push_back(std::move(d));
        }
    }

    auto all_decls = finish();
    std::string json = marshal_declarations(all_decls);

    if (!output_path.empty()) {
        std::string out_file = output_path + "/lifted.json";
        std::ofstream f(out_file);
        if (f) {
            f << json << "\n";
            std::printf("Wrote %zu declarations to %s\n", all_decls.size(), out_file.c_str());
        }
    } else {
        std::cout << json << "\n";
    }
}

}  // namespace

// -----------------------------------------------------------------------
// Main
// -----------------------------------------------------------------------

int main(int argc, char* argv[]) {
    bool rpc_flag = false;
    std::string workspace = ".";
    std::string output = "";

    for (int i = 1; i < argc; i++) {
        std::string arg = argv[i];
        if (arg == "--rpc") {
            rpc_flag = true;
        } else if (arg == "--workspace" && i + 1 < argc) {
            workspace = argv[++i];
        } else if ((arg == "-o" || arg == "--out") && i + 1 < argc) {
            output = argv[++i];
        }
    }

    if (rpc_flag) {
        rpc_mode(workspace);
    } else {
        standalone_mode(workspace, output);
    }

    return 0;
}
