# ProveKit Logging Conventions

## The rule

**Truncating data in logs is forbidden.** The log file captures everything. The stdout view summarizes. Two separate concerns; never conflate.

## Why

Truncation hides the truth at exactly the moment debugging needs it. Repeatedly during ProveKit's development, "we'll just show the first 200 chars" cost hours when the answer was at character 201. Examples that motivated this rule:

- `[llm:claude-agent-sdk] Bash: <command-truncated-at-200>` hid which flags were passed.
- `Edit: file=X len=42` hid what was actually changed.
- `tool_result: first-500-chars` hid whether the result was an error.
- The overlay-bypass bug (Claude editing files outside the scratch worktree via absolute paths) was invisible for an entire afternoon because tool inputs weren't logged in full.

Every one of these would have been instantly visible if we had logged the full data.

## Concrete rules

1. **The persistent log file (`.provekit/fix-loop-<ts>.log`) captures full content.** No truncation. No "preview." No "first N chars." Full input, full output, full reasoning, full tool inputs.

2. **Stdout output may be summarized for live readability.** This is a UX choice, not a data-fidelity choice. The log file is still complete.

3. **Disk pressure is solved by rotation.** Cap the count of `.provekit/fix-loop-*.log` files (e.g., keep last 100). Do not truncate individual log entries to control size.

4. **Sensitive data is solved by structured field-level redaction.** If we need to redact API keys, redact the named field (`apiKey: "[REDACTED]"`). Do not lop the surrounding content.

5. **Search and slicing happen at read time.** `grep`, `jq`, `less`, `awk`. Never at write time.

## What this means in practice

- LLM prompts: log full prompt.
- LLM responses: log full response (including thinking blocks if the SDK exposes them).
- Tool calls: log the tool name AND every parameter in full (Edit's `old_string` and `new_string` complete; Bash's full `command`; Read's `file_path` and content if available).
- Tool results: log the full result body.
- Reasoning text between tool uses: log it.
- Errors: full message, full stack trace.

## How to keep this property

Reviewers should reject any logging code that:
- Calls `.slice(0, N)` on a log payload.
- Says "preview" or "truncated to..." in a logger call.
- Decides at write time what's "too verbose."
- Conflates the stdout view's verbosity setting with what goes to the log file.

Each of those is the same mistake.
