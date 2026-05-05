// SPDX-License-Identifier: Apache-2.0
//
// `provekit-walk-emit`: take a Rust source file, lift + walk + shadow,
// emit a proof.ir bundle as JCS-canonical bytes on stdout.
//
// Usage:
//   provekit-walk-emit <source.rs> <callee_name> <caller_name>
//
// Example:
//   echo '<<bare-demo source>>' > demo.rs
//   provekit-walk-emit demo.rs f main > demo.proof.ir.json
//   blake3sum demo.proof.ir.json   # check matches the bundle's own CID

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use provekit_walk::emit::{shadow_proof_ir_cid, shadow_to_proof_ir};
use provekit_walk::{build_shadow_source, lift_function_precondition, CalleeContract};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: {} <source.rs> <callee_name> <caller_name>", args[0]);
        return ExitCode::from(1);
    }
    let source_path = &args[1];
    let callee_name = &args[2];
    let caller_name = &args[3];

    let src = match fs::read_to_string(source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", source_path, e);
            return ExitCode::from(2);
        }
    };
    let file: syn::File = match syn::parse_str(&src) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error parsing {}: {}", source_path, e);
            return ExitCode::from(3);
        }
    };

    let callee_fn = match find_fn(&file, callee_name) {
        Some(f) => f,
        None => {
            eprintln!("function `{}` not found in {}", callee_name, source_path);
            return ExitCode::from(4);
        }
    };
    let caller_fn = match find_fn(&file, caller_name) {
        Some(f) => f,
        None => {
            eprintln!("function `{}` not found in {}", caller_name, source_path);
            return ExitCode::from(5);
        }
    };

    let pre = lift_function_precondition(&callee_fn);
    let formal_params = all_param_names(&callee_fn);
    let s = build_shadow_source(
        &caller_fn,
        &[CalleeContract {
            callee_name: callee_name.to_string(),
            formal_params,
            precondition: pre,
        }],
    );

    let bytes = shadow_to_proof_ir(&s);
    let cid = shadow_proof_ir_cid(&s);

    eprintln!("# proof.ir bundle for caller={} callee={}", caller_name, callee_name);
    eprintln!("# bundle CID:        {}", cid);
    eprintln!("# bundle bytes:      {}", bytes.len());
    eprintln!("# shadowSource CID:  {}", s.cid);
    eprintln!("# slots:             {}", s.slots.len());
    eprintln!(
        "# arrivals (total):  {}",
        s.slots.iter().map(|sl| sl.arrivals.len()).sum::<usize>()
    );

    let mut stdout = io::stdout().lock();
    if let Err(e) = stdout.write_all(&bytes) {
        eprintln!("error writing stdout: {}", e);
        return ExitCode::from(6);
    }
    let _ = stdout.write_all(b"\n");
    ExitCode::SUCCESS
}

fn find_fn(file: &syn::File, name: &str) -> Option<syn::ItemFn> {
    file.items.iter().find_map(|item| match item {
        syn::Item::Fn(f) if f.sig.ident == name => Some(f.clone()),
        _ => None,
    })
}

fn all_param_names(item_fn: &syn::ItemFn) -> Vec<String> {
    item_fn
        .sig
        .inputs
        .iter()
        .enumerate()
        .map(|(i, arg)| match arg {
            syn::FnArg::Receiver(_) => "__self".to_string(),
            syn::FnArg::Typed(pt) => match &*pt.pat {
                syn::Pat::Ident(p) => p.ident.to_string(),
                // Non-Ident patterns (`_: u32`, `(a, b): ...`, etc.) get a
                // stable positional placeholder so the arity stays aligned
                // with the callee's actual argument count.
                _ => format!("__arg{}", i),
            },
        })
        .collect()
}
