// SPDX-License-Identifier: Apache-2.0
//
// `provekit version`.

use owo_colors::OwoColorize;
use serde_json::json;

use crate::protocol::EXPECTED_CATALOG_CID;
use crate::VersionArgs;

const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(args: VersionArgs) -> u8 {
    if args.out.json {
        let payload = json!({
            "name": "provekit",
            "version": CLI_VERSION,
            "protocolCatalogCid": EXPECTED_CATALOG_CID,
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
    } else {
        println!(
            "{} {} {}",
            "provekit".bold(),
            CLI_VERSION,
            format!("(protocol {EXPECTED_CATALOG_CID})").dimmed()
        );
    }
    crate::EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputFlags;

    #[test]
    fn version_string_present() {
        // The crate version string isn't empty — env! is compile-time.
        assert!(!CLI_VERSION.is_empty());
    }

    #[test]
    fn version_command_exits_ok() {
        let args = VersionArgs {
            out: OutputFlags {
                json: true,
                quiet: false,
            },
        };
        assert_eq!(run(args), crate::EXIT_OK);
    }
}
