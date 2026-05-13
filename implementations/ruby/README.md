# ProvekIt Ruby Kit

This directory is a Ruby implementation of a ProvekIt source kit. It speaks JSON-RPC over stdio, lifts Ruby source into ProofIR-shaped JSON, and realizes that JSON back to Ruby with Ruby sugar modules.

The kit is intentionally language-side only. It does not change Rust, Java, or Python substrate code.

## Run

```sh
bundle exec rspec
```

## Plugin

```sh
bin/provekit-ruby-plugin --rpc
```

The RPC server implements `provekit.plugin.describe`, `provekit.plugin.invoke`, and `provekit.plugin.shutdown`. It also accepts the older `initialize`, `lift`, and `shutdown` names used by `provekit-lift/1` callers.
