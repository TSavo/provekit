// SPDX-License-Identifier: Apache-2.0
//
// `provekit ci ...` — CICP reference admission surface.
//
// Language libraries may emit CICP JSON bodies natively. The Rust CLI
// is the universal checker: it parses the body, runs the reference
// libprovekit validator, and reports the body CID.

use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use libprovekit::ci::check_ci_body;
use owo_colors::OwoColorize;
use serde_json::{json, Value as Json};

use crate::OutputFlags;

#[derive(Parser, Debug, Clone)]
pub struct CiArgs {
    #[command(subcommand)]
    pub cmd: CiCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CiCmd {
    /// Check a CICP body claim and print its canonical CID.
    Check(CiCheckArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct CiCheckArgs {
    /// CICP JSON body claim to validate.
    #[arg(long)]
    pub body: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: CiArgs) -> u8 {
    match args.cmd {
        CiCmd::Check(args) => match run_check(&args) {
            Ok(payload) => {
                if args.out.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| payload.to_string())
                    );
                } else if !args.out.quiet {
                    println!("{}", "ProvekIt CI body check".bold());
                    println!(
                        "  body kind: {}",
                        payload["bodyKind"].as_str().unwrap_or("")
                    );
                    println!("  body CID : {}", payload["bodyCid"].as_str().unwrap_or(""));
                    println!("  status   : {}", "admitted".green().bold());
                }
                crate::EXIT_OK
            }
            Err(e) => {
                eprintln!("{}: {e}", "error".red().bold());
                crate::EXIT_USER_ERROR
            }
        },
    }
}

fn run_check(args: &CiCheckArgs) -> Result<Json, String> {
    let bytes = fs::read(&args.body).map_err(|e| format!("read {}: {e}", args.body.display()))?;
    let body: Json = serde_json::from_slice(&bytes)
        .map_err(|e| format!("parse {}: {e}", args.body.display()))?;
    let check = check_ci_body(&body).map_err(|e| e.to_string())?;

    Ok(json!({
        "kind": "CICheck",
        "ok": true,
        "bodyKind": check.kind,
        "bodyCid": check.cid,
        "bodyPath": args.body.display().to_string(),
    }))
}
