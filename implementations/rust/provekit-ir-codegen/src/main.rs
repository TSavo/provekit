use provekit_ir_codegen::generate_all;

fn main() {
    if let Err(e) = generate_all("../../protocol/provekit-ir.cddl") {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    println!("Success: generated types and compilers in consumer crates");
}
