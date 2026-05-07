// SPDX-License-Identifier: Apache-2.0

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    ExitCode::from(provekit_bug_zoo::run(provekit_bug_zoo::ZooArgs::parse()))
}
