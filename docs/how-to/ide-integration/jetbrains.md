# JetBrains integration (IntelliJ, PyCharm, RubyMine, Rider, CLion, GoLand)

JetBrains IDEs run a Java-based IDE platform with their own LSP support and a plugin marketplace. ProvekIt integrates per-IDE.

## Plugin matrix

| IDE | Kit | Plugin | Status |
|---|---|---|---|
| IntelliJ IDEA | Java | (planned for v1.2) | not yet |
| IntelliJ IDEA | Kotlin | (planned) | not yet |
| PyCharm | Python | "ProvekIt for PyCharm" | shipping |
| RubyMine | Ruby | "ProvekIt for RubyMine" | shipping |
| Rider | C# | "ProvekIt for Rider" | shipping |
| CLion | Rust (via Rust plugin) | "ProvekIt for CLion" | shipping |
| GoLand | Go | (planned for v1.2) | not yet |
| WebStorm | TypeScript | (planned for v1.2) | not yet |
| Android Studio | Java/Kotlin | (planned) | not yet |

> Plugin names and marketplace identifiers are placeholder until the plugins are published. Until then, sideload via `tools/jetbrains/<kit>/build/distributions/*.zip`.

## Installation

When plugins are in the JetBrains Marketplace:

1. Settings → Plugins → Marketplace
2. Search "ProvekIt"
3. Install the plugin matching your IDE
4. Restart

For sideloading (until marketplace publish):

1. Settings → Plugins → ⚙ → Install Plugin from Disk...
2. Select `tools/jetbrains/<kit>/build/distributions/provekit-<kit>-<version>.zip`
3. Restart

## Configuration

Per-IDE settings live in Settings → Tools → ProvekIt:

- **Server path**: path to the LSP plugin binary (auto-detected; override if needed).
- **Protocol version**: `1.1.0` (must match `provekit verify-protocol` output).
- **Diagnostics enabled**: toggle.
- **Tier 3 timeout (ms)**: max time per Tier 3 invocation.

## Verifying it works

Open a file with annotations the kit recognizes. After a brief delay (cold-start), the IDE should show:

- ProvekIt as a problem source in the Problems tool window.
- Inspections highlighting violations.
- Hover tooltips showing the contract.

Use **View → Tool Windows → ProvekIt** (or equivalent) to see the discharge breakdown for the open file.

## Inspections

ProvekIt diagnostics show as IDE inspections. To configure severity per JetBrains conventions:

Settings → Editor → Inspections → ProvekIt. Three categories:

- **Contract violations** (default: ERROR).
- **Tier 3 fallbacks** (default: WARNING).
- **Lifted contracts** (default: INFORMATION).

Each can be enabled / disabled / re-severitied per project.

## Quick fixes

JetBrains IDEs are good at intention actions. ProvekIt plugins ship intentions for:

- "Add annotation to align with caller's contract."
- "Bind to reference contract."
- "Mark this call site Tier 3 (allow solver fallback)."

Use Alt+Enter on a diagnostic to invoke quick fixes.

## Refactoring integration

Changes to annotated code automatically re-trigger lift adapters. No manual refresh required. The IDE's "Continuous Verification" indicator shows whether the workspace is fully verified.

## Workspace boundaries

Each IDE infers the workspace root from its own conventions:

- IntelliJ IDEA / Android Studio: `.iml` files.
- PyCharm: `.idea/` plus `pyproject.toml` / `setup.py`.
- RubyMine: `.idea/` plus `Gemfile`.
- Rider: `.sln` files.
- CLion: `CMakeLists.txt`.
- GoLand: `go.mod`.
- WebStorm: `package.json`.

If lift adapters can't find annotations, check that the workspace root is correct (Settings → Project Structure).

## Troubleshooting

### Plugin doesn't load

- IDE Log: Help → Show Log in Files. Look for `provekit` errors.
- Plugin compatibility: ProvekIt plugins target specific IDE platform versions. Check the plugin's compatibility range against your IDE version.

### Squigglies don't appear

- Confirm `provekit verify-protocol` works from a terminal.
- Confirm the IDE's PATH includes the plugin's binary location.
- Restart the IDE.

### LSP is slow

- Lower `Tier 3 timeout` in Settings.
- Lower `Tier 3 max invocations per parse`.
- Run `provekit prove` from a terminal; if slow there too, the lattice is cold.

## Per-IDE specifics

### PyCharm

Integrates with PyCharm's existing type-checking and inspection infrastructure. ProvekIt diagnostics appear alongside Python type errors.

### RubyMine

Integrates with RubyMine's Ruby type-inference system. ActiveModel and dry-validation annotations are auto-recognized; RSpec contract lifting is supported.

### Rider

Integrates with Rider's .NET infrastructure. DataAnnotations and LINQ predicate quantifiers are auto-recognized.

### CLion

The Rust kit integrates via the JetBrains Rust plugin. Rust-specific lift adapters (proptest, contracts) are auto-loaded.

## Read next

- [overview.md](overview.md).
- [vscode.md](vscode.md): VSCode equivalent.
- [`../../contributing/writing-an-LSP-plugin.md`](../../contributing/writing-an-LSP-plugin.md): porting to other editors.
