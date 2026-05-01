# Agent Plugin Protocol (`provekit-agent/1`)

Status: draft for inclusion in protocol catalog v1.2.0 (the v1.1.0 catalog
is signed and frozen; this spec is RECOMPUTE-pending until v1.2.0 lands).

## Why

ProvekIt is a verification gate for whatever produces contracts. The
producers we want to support are not a fixed set: today it's Claude
Code, Codex, OpenCode, Cursor, Continue, Aider; tomorrow it's
something else. The protocol is the contract that lets any of them
plug in without ProvekIt knowing the binary.

The agent plugin protocol is a thin JSON-RPC seam over stdio (LSP
shape; same shape MCP, nvim plugins, and the language-server
ecosystem use). ProvekIt invokes the agent for proposals; agents
invoke ProvekIt as a tool when they want verification.

This spec defines:

1. The plugin manifest format.
2. The JSON-RPC method shapes.
3. Capability negotiation.
4. The error model.
5. The prompt and configuration surface that wraps both directions.

## Plugin discovery

Plugins live at:

```
~/.config/provekit/agents/<name>/manifest.toml
```

Manifest schema:

```toml
name = "claude-code"
version = "1.0"
protocol_version = "provekit-agent/1"
binary = "provekit-agent-claude-code"   # absolute path or PATH-resolvable
capabilities = ["lift", "must", "fix"]
```

`protocol_version` must match the catalog declared by the running
ProvekIt CLI; mismatch is a hard error reported by `provekit agent list`.

`capabilities` enumerates the methods the plugin implements. ProvekIt
will not call methods missing from this list.

## JSON-RPC methods

All requests are line-delimited JSON-RPC 2.0 over the plugin's stdin;
all responses go to stdout. Any non-JSON output on stdout is a hard
error.

### `provekit.agent.handshake`

The first call. Establishes capability + version compatibility.

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "provekit.agent.handshake",
  "params": {
    "provekit_version": "0.1.0",
    "protocol_version": "provekit-agent/1",
    "catalog_cid": "blake3-512:..."
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "name": "claude-code",
    "version": "1.0",
    "protocol_version": "provekit-agent/1",
    "capabilities": ["lift", "must", "fix"]
  }
}
```

### `provekit.lift.propose`

Read a source file; propose contracts.

Params: `{ source_path, source_text, function_name?, authoring_api_doc, existing_contract_names, previous_rejection? }`.
Result: `[ContractCandidate, ...]`.

### `provekit.must.translate`

Translate one English description.

Params: `{ source_path, source_text, description, authoring_api_doc, previous_rejection? }`.
Result: `ContractCandidate`.

### `provekit.fix.patch`

Patch a bug.

Params: `{ repo_root, bug_description, violated_contracts, allowed_paths, previous_rejection? }`.
Result: `{ patches: [FilePatch, ...], new_contracts: [ContractCandidate, ...], commentary }`.

### `provekit.shutdown`

Graceful close. After this, the plugin process should exit zero on stdin EOF.

## Type shapes

```ts
type ContractCandidate = {
  name: string;
  pre?: string;          // IR-JSON formula as a string
  post?: string;         // ditto
  inv?: string;          // ditto
  out_binding: string;   // default "out"
  provenance: AgentProvenance;
};

type AgentProvenance = {
  agent_name: string;
  agent_version: string;
  model?: string;
  confidence?: number;   // [0.0, 1.0]
  rationale?: string;
};

type FilePatch = {
  path: string;          // repo-relative
  new_content: string;   // full file replacement
  old_content?: string;  // optional precondition
};

type FixResult = {
  patches: FilePatch[];
  new_contracts: ContractCandidate[];
  commentary: string;
};
```

## Error model

JSON-RPC 2.0 error codes plus ProvekIt extensions:

| code  | meaning                                                          |
|-------|------------------------------------------------------------------|
| -32700| Parse error (non-JSON on stdout).                                |
| -32600| Invalid Request (missing fields).                                |
| -32601| Method not found.                                                |
| -32602| Invalid params (validation failed before dispatch).              |
| -32603| Internal error (plugin crashed during handling).                 |
| 1000  | Backend unavailable (auth missing, network down).                |
| 1001  | Generation failed (model returned empty / refused).              |
| 1002  | Rejected by validator; the CLI sends this back via `previous_rejection`. |

## Bidirectional: agents invoke ProvekIt as a tool

Every CLI subcommand supports `--json`. External agents (Claude Code,
Cursor, Continue, Aider) consume `provekit agent describe provekit
--json` to discover ProvekIt's tool surface:

```bash
$ provekit agent describe provekit --json
{
  "provekit_version": "0.1.0",
  "protocol_cid": "blake3-512:...",
  "tools": [
    { "name": "provekit prove", "description": "...", "input_schema": {...}, "output_schema": {...}, "exit_codes": [...] },
    { "name": "provekit hash",  "description": "...", "input_schema": {...} },
    { "name": "provekit must",  "description": "...", "input_schema": {...} },
    { "name": "provekit lift",  "description": "...", "input_schema": {...} },
    { "name": "provekit fix",   "description": "...", "input_schema": {...} },
    { "name": "provekit ask",   "description": "...", "input_schema": {...} }
  ]
}
```

This descriptor is sufficient for any agent framework to register
ProvekIt as a tool.

## Configuration surface

Each project declares its choices in `<project>/.provekit/config.toml`
(user-wide overrides at `~/.config/provekit/config.toml`):

```toml
[authoring]
surface = "ts-zod"   # how the agent should write contracts in this codebase

[authoring.must]     # optional per-command override
surface = "rust-provekit-decorator"

[agent]
backend = "claude-code"
model = "claude-opus-4-7"
api_key_env = "ANTHROPIC_API_KEY"

[agent.fix]          # optional per-command override
backend = "stub"

[solvers]
default = "z3"
# chain     = ["z3", "cvc5"]      # fallback chain (RECOMPUTE; v1.2)
# portfolio = ["z3", "cvc5"]      # parallel mode (RECOMPUTE; v1.2)
# mode      = "first-wins"        # or "consensus"

[solvers.z3]
binary = "z3"
ir_compiler = "smt-lib-v2.6"      # RECOMPUTE; pluggable IR compiler arrives in v1.2.0
```

Auto-detection is **not** part of the protocol. Surface, agent, and
solver are all explicit user configuration. `provekit init` is the
interactive wizard that writes the file.

## Prompt resolution chain

The prompt the CLI hands to the agent is resolved by walking:

1. CLI flag `--prompt-file <path>`.
2. Project per-agent + per-surface: `<project>/.provekit/prompts/<cmd>/<surface>.<agent>.md`.
3. Project per-agent: `<project>/.provekit/prompts/<cmd>/<agent>.md`.
4. Project per-surface: `<project>/.provekit/prompts/<cmd>/<surface>.md`.
5. Project default: `<project>/.provekit/prompts/<cmd>/default.md`.
6. User layers (parallel, in `~/.config/provekit/prompts/`).
7. Bundled defaults (compiled into the CLI via `include_str!`).

First hit wins. Variables substituted with `{{var}}` syntax:

- `{{user_input}}`, `{{source_file_path}}`, `{{source_file_contents}}`
- `{{kit_authoring_api_doc}}`, `{{ir_grammar}}`
- `{{existing_contracts}}`, `{{canonical_examples}}`
- `{{previous_rejection}}`

The bundled prompts teach the agent to author contracts in the
**ProvekIt kit's API**, not raw IR-JSON or SMT-LIB. The wire format
ferries IR-JSON because v1 lacks a host-language kit-code eval path;
v2 will accept kit-source as an alternative wire variant.

## v1 limitations and roadmap

- **Kit-code wire variant**: v1 ferries IR-JSON. v2 (after host-lang
  eval paths exist) accepts kit-source directly.
- **Multi-solver / IR-compiler protocol**: v1 runs Z3 against an
  in-process SMT-LIB emitter. The `[solvers]` config schema captures
  the v2 shape (chain / portfolio / consensus, plus pluggable IR
  compilers per solver dialect). See
  `protocol/specs/2026-04-30-ir-compiler-protocol.md` (RECOMPUTE) for
  the planned `provekit.ir.compile` JSON-RPC shape.
- **Per-surface bundled prompts**: v1 ships `default.md` for each
  command; project-local `<surface>.md` overrides work end-to-end. The
  full bundled per-surface tree (~24 surfaces × 3 commands) is
  scheduled for v1.2.

## Reference plugin

`examples/agent-plugins/echo-agent/echo_agent.py` is a ~80-line Python
plugin that implements every method with canned responses. It serves
as the conformance template for new plugin authors.
