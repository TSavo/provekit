# Menagerie

Menagerie is ProvekIt's executable map of proof workflows.

Each destination starts with an artifact `I`, applies a domain projection `k`,
and checks that the resulting truth claim `t` is carried by signed,
content-addressed evidence:

```text
artifact I -> projection k -> claim t -> signed CID graph
```

Bug Zoo is one destination in that map. It proves that real bug shapes can be
rediscovered as missing edges and accepted only after fixed artifacts close the
same boundary. The rest of the Menagerie names the other routes through the
substrate: long implication chains, supply-chain admission, domain bridges,
protocol migrations, and proof-carrying changes.

## Prerequisites

The runnable exhibits drive real foreign toolchains. The Rust runner shells out
to per-language harness scripts (`./run.sh`) which call `pnpm`, `tsc`, `tsx`,
`javac`, `java`, `mvn`, `dotnet`, `go`, and `jq`. A cold-start visitor missing
any of these will see the exhibit fail with `command not found` from inside
pnpm, Maven, or a harness shell.

Before running an exhibit, run `bash menagerie/scripts/check-prereqs.sh` to
verify the toolchain. The script probes each tool with `command -v`, prints a
PASS/MISSING table, and exits non-zero with a list of the missing tools.

| Tool | Why it is needed | macOS (Homebrew) | Linux (apt or generic) |
|---|---|---|---|
| `cargo` (Rust) | Drives `cargo run --manifest-path menagerie/<exhibit>/Cargo.toml`; implicit if you reached this README via cargo. | `brew install rustup-init && rustup-init` | `curl https://sh.rustup.rs -sSf \| sh` |
| `node` | Hosts `pnpm` and `tsx` for the TypeScript harnesses. | `brew install node` | `apt install nodejs` (or use `nvm`) |
| `pnpm` | Runs `pnpm exec tsc` and `pnpm exec tsx` in the TypeScript bug-zoo harnesses. | `brew install pnpm` | `npm install -g pnpm` (or `corepack enable && corepack prepare pnpm@latest --activate`) |
| `java`, `javac` (JDK 17+) | Compiles and runs the Java bug-zoo lab harnesses and lifter jars. | `brew install openjdk` (link with `sudo ln -sfn $(brew --prefix)/opt/openjdk/libexec/openjdk.jdk /Library/Java/JavaVirtualMachines/openjdk.jdk`) | `apt install default-jdk` |
| `mvn` (Maven) | Builds the `provekit-lift-java-*` jars used by the Java kit-rpc lifters. | `brew install maven` | `apt install maven` |
| `dotnet` (.NET SDK 8+) | Builds and runs the C# bug-zoo harnesses and `Provekit.BugZoo` lifters. | `brew install --cask dotnet-sdk` | `apt install dotnet-sdk-8.0` (or follow Microsoft's package feed) |
| `go` | Runs the Go side of the BZ-SHAPE-007 polyglot link harness. | `brew install go` | `apt install golang` |
| `jq` | Filters JSON in the supply-chain-rails and bridgeworks walkthroughs. | `brew install jq` | `apt install jq` |

The bridgeworks walkthrough scripts also use `cc` and `make` for the C lowerer.
These ship with macOS Command Line Tools and most Linux distributions; install
with `xcode-select --install` on macOS or `apt install build-essential` on
Debian-derived Linux. They are not required by `cargo run --manifest-path
menagerie/bridgeworks/Cargo.toml -- --all`; only by the walkthrough.

## Destinations

| Destination | Claim | Status |
|---|---|---|
| [Bug Zoo](bug-zoo/README.md) | Bugs are missing edges; fixes are closure receipts. | Runnable |
| [Hashbound Mainline](hashbound-mainline/README.md) | Cross-domain implication chains compress to 64-byte verification. | Planned |
| [Supply Chain Rails](supply-chain-rails/README.md) | Authentic compatible-looking releases cannot silently betray preserved contracts. | Runnable |
| [Bridgeworks](bridgeworks/README.md) | ProofIR carries vertical contracts; `.proof` CIDs compress the implication DAG. | Runnable |
| [Protocol Switchyard](protocol-switchyard/README.md) | Protocol versions are roots; migrations are witnessed routing edges. | Planned |
| [Change Station](change-station/README.md) | Commits are `p -> q` proof-carrying transitions. | Planned |

Run the current runnable destination:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all
cargo run --manifest-path menagerie/supply-chain-rails/Cargo.toml -- --all
cargo run --manifest-path menagerie/bridgeworks/Cargo.toml -- --all
```
