// SPDX-License-Identifier: Apache-2.0
//
// provekit-java-self-contracts orchestrator entry point.
//
// Mirrors:
//   implementations/csharp/Provekit.SelfContracts/Program.cs
//   implementations/rust/provekit-self-contracts/src/bin/mint-self-contracts.rs
//   implementations/cpp/provekit-self-contracts/mint_cpp_self_contracts.cpp
//   implementations/go/provekit-self-contracts/cmd/mint-go-self-contracts/main.go
//
// Two modes:
//
//   * Standalone:   java -jar provekit-java-self-contracts.jar [outDir]
//                   Mints once, prints the catalog CID + contractSetCid,
//                   asserts byte-determinism by minting twice into
//                   distinct dirs.
//
//   * --rpc:        java -jar provekit-java-self-contracts.jar --rpc
//                   Speaks the lift-plugin protocol (provekit-lift/1)
//                   over NDJSON on stdio. The Rust CLI dispatcher uses
//                   this mode; spec is
//                   protocol/specs/2026-04-30-lift-plugin-protocol.md.
//
// Authoring surface: java-self-contracts. Slabs walked:
//   - catalog_format    13 contracts mirroring rust catalog_format.rs
//
// Future slabs (lift_plugin_protocol cross-kit bridges, provekit-claim-envelope
// kit-internal invariants) are bootstrapped separately. This module ships
// with a content-meaningful contractSetCid as the first deliverable.

package com.provekit.selfcontracts;

import java.util.Arrays;

public final class Main {

    private Main() {}

    public static void main(String[] args) throws Exception {
        if (Arrays.asList(args).contains("--rpc")) {
            Rpc.run();
            return;
        }

        String outDir = (args.length >= 1) ? args[0] : ".";
        Orchestrator.runCli(outDir);
    }
}
