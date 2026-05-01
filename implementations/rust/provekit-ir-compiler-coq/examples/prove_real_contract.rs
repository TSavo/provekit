use provekit_ir_compiler_coq::CoqCompiler;
use provekit_ir_compiler::IrCompiler;
use serde_json::json;

fn main() {
    let compiler = CoqCompiler::new();
    
    // Use one of the actual kit invariants - varterm_no_sort_field
    // This contract: forall x:String. roundTrips(make_var("testvar"))
    let ir = json!({
        "kind": "forall",
        "name": "x",
        "sort": {"kind": "primitive", "name": "String"},
        "body": {
            "kind": "atomic",
            "name": "roundTrips",
            "args": [{"kind": "var", "name": "testvar"}]
        }
    });
    
    println!("=== Compiling IR Contract ===\n");
    println!("IR: {:?}", ir);
    println!();
    
    let result = compiler.compile(&ir, "coq").unwrap();
    
    println!("=== Coq Output ===\n");
    println!("{}", result.preamble);
    println!("{}", result.body);
    println!();
    println!("=== Free Variables ===");
    for fv in &result.free_vars {
        println!("  {} : {}", fv.name, fv.sort);
    }
    
    // Write to file for coqc
    let full_output = format!("{}{}", result.preamble, result.body);
    std::fs::write("/tmp/test_contract.v", &full_output).unwrap();
    println!("\n=== Saved to /tmp/test_contract.v ===");
}