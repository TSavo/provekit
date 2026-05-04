// SPDX-License-Identifier: Apache-2.0
//
// `provekit prove` / `provekit verify` — runs the six-stage pipeline.

use std::path::PathBuf;

use owo_colors::OwoColorize;
use provekit_verifier::{Runner, RunnerConfig};

use crate::report_fmt;
use crate::ProveArgs;

pub fn run(args: ProveArgs) -> u8 {
    let project_root: PathBuf = args.project.unwrap_or_else(|| PathBuf::from("."));
    if !project_root.exists() {
        eprintln!(
            "{}: project root does not exist: {}",
            "error".red().bold(),
            project_root.display()
        );
        return crate::EXIT_USER_ERROR;
    }

    let extra_projects: Vec<PathBuf> = args
        .with
        .iter()
        .map(PathBuf::from)
        .collect();

    let cfg = RunnerConfig {
        project_root: project_root.clone(),
        z3_path: args.z3,
        extra_projects,
        ..Default::default()
    };
    let runner = Runner::new(cfg);
    let report = runner.run();

    if args.out.json {
        let j = report_fmt::report_to_json(&report);
        match serde_json::to_string_pretty(&j) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("{}: serialize JSON: {e}", "error".red().bold());
                return crate::EXIT_USER_ERROR;
            }
        }
    } else {
        report_fmt::print_report_pretty(&report, args.out.quiet);
    }

    report_fmt::report_exit_code(&report)
}
