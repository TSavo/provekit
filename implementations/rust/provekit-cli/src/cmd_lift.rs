// SPDX-License-Identifier: Apache-2.0
//
// `provekit lift <FILE>`. Stub for v0 — the lift adapter lives in TS.

use owo_colors::OwoColorize;
use serde_json::json;

use crate::LiftArgs;

pub fn run(args: LiftArgs) -> u8 {
    let msg = "Lift v0 lives in TS. See implementations/typescript/src/proveLift/. \
               Coming to Rust in v1.2.0.";
    if args.out.json {
        let payload = json!({
            "status": "stub",
            "message": msg,
            "file": args.file.as_ref().map(|p| p.display().to_string()),
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
    } else {
        println!("{} {}", "lift:".yellow().bold(), msg);
    }
    crate::EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputFlags;

    #[test]
    fn lift_returns_ok() {
        let args = LiftArgs {
            file: None,
            out: OutputFlags::default(),
        };
        assert_eq!(run(args), crate::EXIT_OK);
    }
}
