#include "cpp_source_lifter.hpp"

#include <iostream>
#include <string>

int main(int argc, char** argv) {
    for (int i = 1; i < argc; ++i) {
        if (std::string(argv[i]) == "--rpc") return provekit::cpp_source::run_rpc();
    }
    std::cerr << "usage: provekit-lift-cpp-source --rpc\n";
    return 2;
}
