// SPDX-License-Identifier: Apache-2.0
//
// `provekit init [PROJECT]`.
//
// Creates:
//   <PROJECT>/provekit.toml
//   <PROJECT>/.provekit/cache/.gitkeep
//   <PROJECT>/.provekit/sample.invariant.rs
//   <PROJECT>/.github/workflows/provekit.yml

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use owo_colors::OwoColorize;

use crate::InitArgs;

pub fn run(args: InitArgs) -> u8 {
    let project = args.project.unwrap_or_else(|| PathBuf::from("."));
    match init_project(&project, args.force, args.out.quiet) {
        Ok(()) => crate::EXIT_OK,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

pub(crate) fn init_project(project: &Path, force: bool, quiet: bool) -> Result<()> {
    if !project.exists() {
        std::fs::create_dir_all(project)
            .with_context(|| format!("create project dir {}", project.display()))?;
    }

    let toml_path = project.join("provekit.toml");
    write_if_absent_or_force(
        &toml_path,
        &provekit_toml_template(),
        force,
        "provekit.toml",
    )?;

    let cache_dir = project.join(".provekit").join("cache");
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("create {}", cache_dir.display()))?;
    let gitkeep = cache_dir.join(".gitkeep");
    write_if_absent_or_force(&gitkeep, "", force, ".provekit/cache/.gitkeep")?;

    let sample = project.join(".provekit").join("sample.invariant.rs");
    write_if_absent_or_force(
        &sample,
        SAMPLE_INVARIANT_RS,
        force,
        ".provekit/sample.invariant.rs",
    )?;

    let wf_dir = project.join(".github").join("workflows");
    std::fs::create_dir_all(&wf_dir).with_context(|| format!("create {}", wf_dir.display()))?;
    let wf_path = wf_dir.join("provekit.yml");
    write_if_absent_or_force(
        &wf_path,
        GITHUB_ACTION_TEMPLATE,
        force,
        ".github/workflows/provekit.yml",
    )?;

    if !quiet {
        println!("{}", "Initialized ProvekIt project".green().bold());
        println!("  root          : {}", project.display());
        println!("  provekit.toml : created");
        println!("  next steps    :");
        println!("    1. Author your first contract under .provekit/");
        println!("    2. Run `provekit prove` to verify the proof catalog.");
    }
    Ok(())
}

fn write_if_absent_or_force(path: &Path, contents: &str, force: bool, label: &str) -> Result<()> {
    if path.exists() && !force {
        return Err(anyhow!(
            "{} already exists at {} (use --force to overwrite)",
            label,
            path.display()
        ));
    }
    std::fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn provekit_toml_template() -> String {
    r#"# ProvekIt project manifest.

[paths]
# Where signed .proof catalogs live. Walked recursively by `provekit prove`.
proofs = "."

# Where minted artifacts (cached implication mementos, etc.) are stored.
cache = ".provekit/cache"

[solver]
# Path to z3 binary; "z3" defers to PATH.
z3 = "z3"
"#
    .to_string()
}

const SAMPLE_INVARIANT_RS: &str = r#"// Sample ProvekIt invariant (Rust). Author with the kit primitives.
//
// use provekit_ir_symbolic::{Int, forall, gt, must, num};
//
// must("parseInt", forall(Int(), |n| gt(n, num(0))));
"#;

const GITHUB_ACTION_TEMPLATE: &str = r#"name: provekit
on:
  push:
    branches: [ main ]
  pull_request:
jobs:
  prove:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: install z3
        run: sudo apt-get update && sudo apt-get install -y z3
      - name: install provekit
        run: cargo install --git https://github.com/provekit/provekit provekit-cli
      - name: prove
        run: provekit prove .
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_expected_files() {
        let dir =
            std::env::temp_dir().join(format!("provekit-cli-init-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        init_project(&dir, false, true).expect("init");
        assert!(dir.join("provekit.toml").exists());
        assert!(dir.join(".provekit").join("cache").exists());
        assert!(dir.join(".provekit").join("sample.invariant.rs").exists());
        assert!(dir
            .join(".github")
            .join("workflows")
            .join("provekit.yml")
            .exists());

        // Re-running without --force fails.
        let again = init_project(&dir, false, true);
        assert!(again.is_err());

        // With --force, succeeds.
        init_project(&dir, true, true).expect("force re-init");

        std::fs::remove_dir_all(&dir).ok();
    }
}
