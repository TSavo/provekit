# Neovim integration

Neovim's LSP support is built around `nvim-lspconfig`. Sugar LSP plugins integrate as standard LSP servers.

## Install via nvim-lspconfig

Plugin manager-agnostic. Add a config entry per Sugar kit:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.sugar_rust then
  configs.sugar_rust = {
    default_config = {
      cmd = { 'sugar-lsp-rust' },
      filetypes = { 'rust' },
      root_dir = lspconfig.util.root_pattern('Cargo.toml'),
      settings = {
        sugar = {
          protocolVersion = '1.1.0',
          tier3Timeout = 5000,
        },
      },
    },
  }
end

lspconfig.sugar_rust.setup{}
```

Equivalent for other kits:

```lua
-- Python
configs.sugar_python = {
  default_config = {
    cmd = { 'sugar-lsp-py' },
    filetypes = { 'python' },
    root_dir = lspconfig.util.root_pattern('pyproject.toml', 'setup.py'),
    settings = { sugar = { protocolVersion = '1.1.0' } },
  },
}
lspconfig.sugar_python.setup{}

-- Zig
configs.sugar_zig = {
  default_config = {
    cmd = { 'sugar-lift-zig', '--rpc' },
    filetypes = { 'zig' },
    root_dir = lspconfig.util.root_pattern('build.zig'),
    settings = { sugar = { protocolVersion = '1.1.0' } },
  },
}
lspconfig.sugar_zig.setup{}

-- Ruby
configs.sugar_ruby = {
  default_config = {
    cmd = { 'sugar-lsp-ruby' },
    filetypes = { 'ruby' },
    root_dir = lspconfig.util.root_pattern('Gemfile'),
    settings = { sugar = { protocolVersion = '1.1.0' } },
  },
}
lspconfig.sugar_ruby.setup{}

-- C#
configs.sugar_csharp = {
  default_config = {
    cmd = { 'sugar-lsp-csharp' },
    filetypes = { 'cs' },
    root_dir = lspconfig.util.root_pattern('*.csproj', '*.sln'),
    settings = { sugar = { protocolVersion = '1.1.0' } },
  },
}
lspconfig.sugar_csharp.setup{}
```

## Diagnostic display

Use whichever diagnostic viewer you have installed. Recommended:

- `vim.diagnostic.config({ virtual_text = true })` for inline diagnostic display.
- `vim.diagnostic.config({ signs = true })` for gutter signs.
- A floating-window diagnostic viewer like `lsp_lines.nvim` or `trouble.nvim` for verbose messages.

## Filtering Sugar diagnostics

Sugar diagnostics use source `"sugar"`. Filter for "show only Sugar issues":

```vim
:lua require('telescope.builtin').diagnostics({ source = 'sugar' })
```

Or in your config:

```lua
vim.api.nvim_create_user_command('SugarDiagnostics', function()
  local diagnostics = vim.diagnostic.get(0, {
    severity = { vim.diagnostic.severity.ERROR, vim.diagnostic.severity.WARN }
  })
  local sugar_only = vim.tbl_filter(function(d) return d.source == 'sugar' end, diagnostics)
  vim.diagnostic.setqflist({ open = true, items = sugar_only })
end, {})
```

## Performance tuning

If the LSP is slow, tune in your config:

```lua
lspconfig.sugar_rust.setup{
  settings = {
    sugar = {
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
lspconfig.sugar_rust.setup{}
lspconfig.sugar_python.setup{}
lspconfig.sugar_zig.setup{}
lspconfig.sugar_ruby.setup{}
lspconfig.sugar_csharp.setup{}
```

When TypeScript / Go / C++ / Java LSP plugins ship, add their configs analogously.

## Troubleshooting

### LSP doesn't start

- Check `:LspInfo` in command mode. The LSP should appear as registered.
- Check `:LspLog` for startup errors.
- Verify `sugar-lsp-rust` (or equivalent) is on PATH: `:echo system("which sugar-lsp-rust")`.
- Verify `sugar verify-protocol` runs successfully from a terminal.

### Squigglies don't appear

- Check `:lua print(vim.inspect(vim.diagnostic.get(0)))`. If the table is empty, the LSP isn't producing diagnostics.
- Verify the file is the right type (`:set ft?`).
- Verify you're in a workspace where lift adapters can find annotations.

### LSP is slow

- Check `:LspLog` for repeated Tier 3 fallback log entries.
- Lower `tier3Timeout` and `tier3MaxInvocationsPerParse` (see above).
- Run `sugar prove` from a terminal; if it's slow there too, the lattice is cold.

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
