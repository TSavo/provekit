// SPDX-License-Identifier: Apache-2.0

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    ExitCode::from(provekit_supply_chain_rails::run(
        provekit_supply_chain_rails::SupplyChainRailsArgs::parse(),
    ))
}
