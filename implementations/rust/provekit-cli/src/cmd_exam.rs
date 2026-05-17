// SPDX-License-Identifier: Apache-2.0
//
// `provekit exam show <PATH-OR-CID>`: load an ExamManifestMemento via the
// PEP 1.7.0 exam-manifest dispatch surface, validate it, and print a
// summary to stdout.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use serde_json::json;

use crate::kit_dispatch::dispatch_exam_manifest;
use crate::OutputFlags;

#[derive(Parser, Debug, Clone)]
pub struct ExamArgs {
    #[command(subcommand)]
    pub cmd: ExamCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ExamCmd {
    /// Load and summarize an ExamManifestMemento.
    Show(ExamShowArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ExamShowArgs {
    /// Local manifest path or BLAKE3-512 catalog CID.
    pub path_or_cid: String,
    /// Exam-manifest plugin name. Falls back to the built-in loader when absent.
    #[arg(long, default_value = "default")]
    pub plugin: String,
    /// Workspace root used for plugin discovery and relative paths.
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: ExamArgs) -> u8 {
    match args.cmd {
        ExamCmd::Show(args) => run_show(args),
    }
}

fn run_show(args: ExamShowArgs) -> u8 {
    let workspace_root = args.project.unwrap_or_else(|| PathBuf::from("."));
    match dispatch_exam_manifest(&workspace_root, &args.plugin, &args.path_or_cid) {
        Ok(manifest) => {
            if !args.out.quiet {
                print_summary(&manifest, args.out.json);
            }
            crate::EXIT_OK
        }
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

fn print_summary(manifest: &provekit_ir_types::ExamManifestMemento, json_out: bool) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for question in &manifest.header.content.questions {
        *counts
            .entry(question.kind.as_str().to_string())
            .or_default() += 1;
    }
    let total = manifest.header.content.questions.len();

    if json_out {
        let payload = json!({
            "manifest_cid": manifest.header.cid,
            "concept_hub_version": manifest.header.content.concept_hub_version,
            "questions": total,
            "question_counts": counts,
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        return;
    }

    println!("manifest_cid: {}", manifest.header.cid);
    println!(
        "concept_hub_version: {}",
        manifest.header.content.concept_hub_version
    );
    println!("questions: {total}");
    for (kind, count) in counts {
        println!("  {kind}: {count}");
    }
}
