//! Cross-language equivalence runner: Rust path.
//!
//! Usage: cargo run -- <fixture-name>
//! Emits: compact JSON of the Declaration[] for the named fixture.

use provekit_ir_symbolic::property::{begin_collecting, property as property_fn, _reset_collector};
use provekit_ir_symbolic::types::sorts;
use provekit_ir_symbolic::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: cross-lang-runner <fixture-name>");
        std::process::exit(2);
    }
    let fixture = &args[1];

    _reset_collector();
    let handle = begin_collecting();

    match fixture.as_str() {
        "forall_int_gt_zero" => {
            property_fn(
                "forall_int_gt_zero",
                forall!(x: sorts::int() => gt(x, num(0_i64))),
            );
        }
        "eq_parseint_zero_zero" => {
            property_fn(
                "eq_parseint_zero_zero",
                eq(parse_int(str_("0")), num(0_i64)),
            );
        }
        "forall_string_parseint_gte_zero" => {
            property_fn(
                "forall_string_parseint_gte_zero",
                forall!(s: sorts::string() => gte(parse_int(s), num(0_i64))),
            );
        }
        _ => {
            eprintln!("unknown fixture: {}", fixture);
            std::process::exit(2);
        }
    };

    let decls = handle.finish();
    print!("{}", serde_json::to_string(&decls).unwrap());
}
