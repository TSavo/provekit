# Neovim integration

Neovim's LSP support is built around `nvim-lspconfig`. Sugar LSP plugins integrate as standard LSP servers.

## Install via nvim-lspconfig

Plugin manager-agnostic. Add a config entry per Sugar kit:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.provekit_rust then
  configs.provekit_rust = {
    default_config = {
      cmd = { 'provekit-lsp-rust' },
      filetypes = { 'rust' },
      root_dir = lspconfig.util.root_pattern('Cargo.toml'),
      settings = {
        provekit = {
          protocolVersion = '1.1.0',
          tier3Timeout = 5000,
        },
      },
    },
  }
end

lspconfig.provekit_rust.setup{}
```

Equivalent for other kits:

```lua
-- Python
configs.provekit_python = {
  default_config = {
    cmd = { 'provekit-lsp-py' },
    filetypes = { 'python' },
    root_dir = lspconfig.util.root_pattern('pyproject.toml', 'setup.py'),
    settings = { provekit = { protocolVersion = '1.1.0' } },
  },
}
lspconfig.provekit_python.setup{}

-- Zig
configs.provekit_zig = {
  default_config = {
    cmd = { 'provekit-lift-zig', '--rpc' },
    filetypes = { 'zig' },
    root_dir = lspconfig.util.root_pattern('build.zig'),
    settings = { provekit = { protocolVersion = '1.1.0' } },
  },
}
lspconfig.provekit_zig.setup{}

-- Ruby
configs.provekit_ruby = {
  default_config = {
    cmd = { 'provekit-lsp-ruby' },
    filetypes = { 'ruby' },
    root_dir = lspconfig.util.root_pattern('Gemfile'),
    settings = { provekit = { protocolVersion = '1.1.0' } },
  },
}
lspconfig.provekit_ruby.setup{}

-- C#
configs.provekit_csharp = {
  default_config = {
    cmd = { 'provekit-lsp-csharp' },
    filetypes = { 'cs' },
    root_dir = lspconfig.util.root_pattern('*.csproj', '*.sln'),
    settings = { provekit = { protocolVersion = '1.1.0' } },
  },
}
lspconfig.provekit_csharp.setup{}
```

## Diagnostic display

Use whichever diagnostic viewer you have installed. Recommended:

- `vim.diagnostic.config({ virtual_text = true })` for inline diagnostic display.
- `vim.diagnostic.config({ signs = true })` for gutter signs.
- A floating-window diagnostic viewer like `lsp_lines.nvim` or `trouble.nvim` for verbose messages.

## Filtering Sugar diagnostics

Sugar diagnostics use source `"provekit"`. Filter for "show only Sugar issues":

```vim
:lua require('telescope.builtin').diagnostics({ source = 'provekit' })
```

Or in your config:

```lua
vim.api.nvim_create_user_command('ProvekitDiagnostics', function()
  local diagnostics = vim.diagnostic.get(0, {
    severity = { vim.diagnostic.severity.ERROR, vim.diagnostic.severity.WARN }
  })
  local provekit_only = vim.tbl_filter(function(d) return d.source == 'provekit' end, diagnostics)
  vim.diagnostic.setqflist({ open = true, items = provekit_only })
end, {})
```

## Performance tuning

If the LSP is slow, tune in your config:

```lua
lspconfig.provekit_rust.setup{
  settings = {
    provekit = {
      protocolVersion = '1.1.0',
      tier3Timeout = 2000,           -- fail Tier 3 faster
      tier3MaxInvocationsPerParse = 3, -- cap Tier 3 calls per parse
      cacheSize = 10000,
    },
  },
}
```

## Multiple kits in one project

A polyglot project (e.g., a Rust backend with Python ML and TypeScript frontend) runs multiple Sugar LSPs simultaneously. Each `setup{}` registers a separate LSP. Filetype routing handles which LSP processes which file.

```lua
-- All shipping Sugar kits, in one config
lspconfig.provekit_rust.setup{}
lspconfig.provekit_python.setup{}
lspconfig.provekit_zig.setup{}
lspconfig.provekit_ruby.setup{}
lspconfig.provekit_csharp.setup{}
```

When TypeScript / Go / C++ / Java LSP plugins ship, add their configs analogously.

## Troubleshooting

### LSP doesn't start

- Check `:LspInfo` in command mode. The LSP should appear as registered.
- Check `:LspLog` for startup errors.
- Verify `provekit-lsp-rust` (or equivalent) is on PATH: `:echo system("which provekit-lsp-rust")`.
- Verify `provekit verify-protocol` runs successfully from a terminal.

### Squigglies don't appear

- Check `:lua print(vim.inspect(vim.diagnostic.get(0)))`. If the table is empty, the LSP isn't producing diagnostics.
- Verify the file is the right type (`:set ft?`).
- Verify you're in a workspace where lift adapters can find annotations.

### LSP is slow

- Check `:LspLog` for repeated Tier 3 fallback log entries.
- Lower `tier3Timeout` and `tier3MaxInvocationsPerParse` (see above).
- Run `provekit prove` from a terminal; if it's slow there too, the lattice is cold.

## Per-language specifics

### Rust

The Rust LSP is most often run alongside `rust-analyzer`. Both publish diagnostics; configure your diagnostic viewer to show both, filtered by source if you want to disambiguate.

### Python

The Python LSP works alongside `pylsp` or `pyright`. Sugar focuses on contracts; the type checker handles types.

### Zig

The Zig LSP integrates with `zls` (Zig Language Server). Both run; both publish diagnostics.

### Ruby

The Ruby LSP works alongside `solargraph` (when available).

### C#

The C# LSP works alongside `omnisharp` or `csharp-ls`.

## Read next

- [overview.md](overview.md).
- [vscode.md](vscode.md): VSCode equivalent.
- [`../../contributing/writing-an-LSP-plugin.md`](../../contributing/writing-an-LSP-plugin.md): porting to other editors.
