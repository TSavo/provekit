// Cross-language equivalence runner — C++ path.
//
// Usage: ./cross-lang-runner <fixture-name>
// Emits: compact JSON of the Declaration[] for the named fixture.

#include <cstdlib>
#include <cstring>
#include <iostream>
#include <string>

#include "provekit/ir.hpp"

namespace ir = provekit::ir;

int main(int argc, char* argv[]) {
  if (argc < 2) {
    std::cerr << "usage: cross-lang-runner <fixture-name>\n";
    return 2;
  }
  std::string fixture = argv[1];

  ir::reset_collector();
  ir::begin_collecting();

  if (fixture == "forall_int_gt_zero") {
    ir::property(
      "forall_int_gt_zero",
      ir::forall(ir::Int(), [](std::shared_ptr<ir::Term> x) {
        return ir::gt(x, ir::num(0));
      }));
  } else {
    std::cerr << "unknown fixture: " << fixture << "\n";
    return 2;
  }

  auto decls = ir::finish();
  std::cout << ir::marshal_declarations(decls);
  return 0;
}
