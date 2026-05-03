# Quickstart: Get a red squiggle in 10 minutes

You write Rust code with a `#[requires(n > 0)]` precondition. Your colleague calls your Rust function from Go via cgo. You want their IDE to show a red squiggle when their Go caller passes a value that violates your Rust precondition. This walkthrough delivers exactly that.

Build time for the Rust workspace is several minutes on a cold machine. The ten minutes below count from when the binaries are on PATH.

## Prerequisites

- Rust toolchain (rustup, stable channel)
- Go 1.22 or later
- An LSP-capable editor: VSCode, neovim, Helix, IntelliJ, or any editor with LSP support
- The ProvekIt repo cloned locally (the demo uses the `examples/polyglot-rust-go/` fixture already in the repo)

## Step 1: build and install the binaries

From the repo root:

```sh
# Build the main CLI
cargo install --path implementations/rust/provekit-cli

# Build the LSP server
cargo install --path implementations/rust/provekit-lsp

# Build the linker daemon
cargo install --path implementations/rust/provekit-linkerd
```

All three binaries install to `~/.cargo/bin/`. Confirm they are on your PATH:

```sh
provekit --version
provekit-lsp --version
provekit-linkerd --help
```

## Step 2: configure your editor

The LSP server (`provekit-lsp`) connects to the linker daemon (`provekit-linkerd`) at a Unix domain socket. Pass the socket path via `--daemon-socket`. The daemon spawns itself on first connection; you do not start it separately.

The socket path follows the daemon spec: `${XDG_RUNTIME_DIR}/provekit/linkerd-<projectCid>.sock`. For the demo, use a fixed path:

```
/tmp/provekit-demo.sock
```

### VSCode

There is no dedicated ProvekIt VSCode extension yet. Use a generic LSP client extension such as `lsp-client` (extension ID `matklad.lsp-client`) or `multi-lsp-client`. Configure it with:

- Executable: `provekit-lsp`
- Arguments: `--daemon-socket /tmp/provekit-demo.sock`

The server speaks standard LSP over stdio. If your generic LSP extension uses a JSON config block, the fields are whatever that extension defines -- look at its own docs for the exact key names.

### neovim

Add to your `init.lua` or a project-local config:

```lua
vim.lsp.start({
  name = "provekit",
  cmd = { "provekit-lsp", "--daemon-socket", "/tmp/provekit-demo.sock" },
  root_dir = vim.fn.getcwd(),
  filetypes = { "rust", "go" },
})
```

### Helix

In `.helix/languages.toml`:

```toml
[[language]]
name = "rust"
language-servers = ["provekit-lsp", "rust-analyzer"]

[[language]]
name = "go"
language-servers = ["provekit-lsp", "gopls"]

[language-server.provekit-lsp]
command = "provekit-lsp"
args = ["--daemon-socket", "/tmp/provekit-demo.sock"]
```

### IntelliJ

Install the LSP4IJ plugin. Add an external language server:

- Program: `provekit-lsp`
- Arguments: `--daemon-socket /tmp/provekit-demo.sock`
- File types: Rust, Go

## Step 3: run the demo

The `examples/polyglot-rust-go/` directory contains two fixtures: `fixture-fail/` and `fixture-ok/`.

**fixture-fail:** A Go caller (`caller_fail.go`) calls a Rust function via cgo. The Rust function declares `#[requires(n > 0)]`. The Go caller passes `n` directly without any guard, so the linker cannot discharge the precondition obligation.

**fixture-ok:** A Go caller (`caller_ok.go`) guards the input before using it, so the contract is established.

Run the linker pass on the failure fixture:

```sh
provekit link examples/polyglot-rust-go/fixture-fail/
```

You should see at least one linker error reported on stderr (an unprovable obligation from the Go caller to the Rust function's precondition). The command exits with a non-zero exit code. A `link-bundle.json` is written to the fixture directory recording the full derivation.

Run the linker pass on the success fixture:

```sh
provekit link examples/polyglot-rust-go/fixture-ok/
```

You should see zero linker errors and the command exits with code 0. The `link-bundle.json` written here carries the clean bundle CID.

## Step 4: see the red squiggle

Open `examples/polyglot-rust-go/fixture-fail/go-caller/caller_fail.go` in your editor with the LSP server running. The LSP server forwards the file to the daemon, which runs the linker pass and returns the diagnostic. Your editor should show a diagnostic (red squiggle or warning annotation) indicating an unprovable cross-language obligation from the Go caller to the Rust function's precondition.

Open `examples/polyglot-rust-go/fixture-ok/go-caller/caller_ok.go`. No diagnostic. The guard the Go caller adds establishes the postcondition, and the linker discharges the obligation cleanly.

Note: per the current daemon MVP, diagnostics are attached at line 0 (file-level marker) because call-site locus propagation from the linker to the LSP is a follow-up item. The squiggle appears at the top of the file. Precise line-level squiggles are on the roadmap.

## Step 5: try it on your own project

1. Add a `provekit.config.yaml` to your project root (run `provekit init` to generate a template).
2. Annotate your Rust functions with `#[requires(...)]` / `#[ensures(...)]` (from the `contracts` crate) or use `assert!` predicates in the function body. ProvekIt's lifter reads both.
3. For Go callers, add `//provekit:contract` above your Go functions that call into Rust via cgo.
4. Run `provekit link <project-root>` to see the linker pass.
5. Point your editor at `provekit-lsp --daemon-socket <socket-path>` and open source files.

## When something goes wrong

**`provekit: command not found`**
The install did not add `~/.cargo/bin` to your PATH. Add `export PATH="$HOME/.cargo/bin:$PATH"` to your shell config and reload.

**`provekit-linkerd: socket permission denied`**
The daemon socket is owned by a different user. Delete `/tmp/provekit-demo.sock` and restart.

**`daemon-client: failed to connect`**
The daemon failed to start. Run `provekit-linkerd --socket /tmp/provekit-demo.sock` in a terminal to see the error output. Common cause: `provekit-linkerd` not on PATH.

**`provekit link` exits with `go-lsp-bin not found`**
The Go kit lifter (`provekit-lsp-go`) is not on PATH. Build it: `cd implementations/go && go build ./...`. The resulting `provekit-lsp-go` binary goes to your PATH.

**Zero linker errors but no red squiggles in the editor**
The LSP server is not connected. Confirm `provekit-lsp --version` works from your shell and that your editor's LSP config points at the binary name that actually exists on PATH.

## What is next

The demo uses the rust+go cross-kit path. The same architecture covers any language pair the daemon's kit dispatch supports. See [docs/per-language-status.md](per-language-status.md) for the current matrix of kits with LSP plugin support.

If you want to understand the architecture, write a new lifter, or contribute a new kit, the extender quickstart is [docs/quickstart-extender.md](quickstart-extender.md).
