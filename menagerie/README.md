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

## Destinations

| Destination | Claim | Status |
|---|---|---|
| [Bug Zoo](bug-zoo/README.md) | Bugs are missing edges; fixes are closure receipts. | Runnable |
| [Hashbound Mainline](hashbound-mainline/README.md) | Cross-domain implication chains compress to 64-byte verification. | Planned |
| [Supply Chain Rails](supply-chain-rails/README.md) | Artifacts stay on verified tracks through rank-3 pins, `binaryCid`, and CICP. | Planned |
| [Bridgeworks](bridgeworks/README.md) | Proof survives domain crossings through content-addressed bridges. | Planned |
| [Protocol Switchyard](protocol-switchyard/README.md) | Protocol versions are roots; migrations are witnessed routing edges. | Planned |
| [Change Station](change-station/README.md) | Commits are `p -> q` proof-carrying transitions. | Planned |

Run the current runnable destination:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all
```
