// SPDX-License-Identifier: Apache-2.0

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    ExitCode::from(provekit_bridgeworks::run(
        provekit_bridgeworks::BridgeworksArgs::parse(),
    ))
}
