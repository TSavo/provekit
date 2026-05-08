use provekit_ir_compiler::IrCompiler;
use provekit_ir_compiler_coq::CoqCompiler;
use serde_json::json;

fn main() {
    let mut compiler = CoqCompiler::new();

    // Simple formula
    let ir = json!({
        "kind": "atomic",
        "name": "roundTrips",
        "args": [{"kind": "var", "name": "s"}]
    });

    let result = compiler.compile(&ir, "coq").unwrap();

    println!("=== PREAMBLE ===");
    println!("{}", result.preamble);
    println!("=== BODY ===");
    println!("{}", result.body);
    println!("=== FREE VARS ===");
    println!("{:?}", result.free_vars);
}
