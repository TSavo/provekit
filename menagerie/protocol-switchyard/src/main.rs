// SPDX-License-Identifier: Apache-2.0

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    ExitCode::from(provekit_protocol_switchyard::run(
        provekit_protocol_switchyard::SwitchyardArgs::parse(),
    ))
}
