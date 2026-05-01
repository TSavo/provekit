// SPDX-License-Identifier: Apache-2.0
//
// `provekit-lift` — direct invocation entry point.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flags = provekit_lift::parse_cli_flags(args);
    let code = provekit_lift::run_cli(flags);
    std::process::exit(code);
}
