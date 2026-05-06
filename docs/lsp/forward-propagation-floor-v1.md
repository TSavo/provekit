# Forward-Propagation v1.0.0 Floor Spec

This document defines the scope for the LSP forward-propagator implementation across all kits.

## IN scope at v1.0.0 floor

- **Variable assignment posts**: `let x = expr` accumulates a post `typeof(x)` or the callee's post for the assigned callee
- **Sequential flow**: posts accumulate left-to-right in a basic block
- **If/else branch merge**: uses G3 disjunction (per #256) to merge the two branch posts
- **Function call posts**: when the callee has a known post in the seed catalog, that post is added to the accumulated post
- **Callsite pre-check**: for callsites with a known callee pre (from seed catalog via #312 index), emit a verifier query `current_post implies callee_pre`; failed implication produces a diagnostic per #311
- **`top` fallback**: any construct not in this list maps the propagator's post to `top` for the current scope

## OUT of scope (defer to v1.1+)

- **Loops**: post is `top` at loop entry
- **Closures**: post is `top` at capture site
- **Dynamic dispatch**: post is `top` when callee is not statically resolved
- **Complex aliasing**: no alias analysis; each variable post is tracked independently
- **Inter-procedural analysis**: beyond the single-function body
- **Exception propagation**: post at `?`/`throw`/`raise`/`panic` is `top`

## `top` semantics

When the propagator cannot track a post precisely, it uses `top` (the weakest possible post: no predicates). A callsite pre-check against `top` always fails implication. The propagator MUST NOT emit a false-positive diagnostic for `top`-fallback paths; it must suppress the diagnostic when the accumulated post is `top`.

## Cross-kit applicability

Every per-kit issue (#313-#324) must implement exactly this scope. Deviating from the IN/OUT lists requires a new floor spec version, not a per-kit exception.

## Diagnostic shape (per #311)

Diagnostics follow the LSP protocol:

```json
{
  "range": { "start": { "line": N, "character": M }, "end": { "line": N, "character": M + L } },
  "severity": 1,
  "code": "implication-failed",
  "source": "provekit",
  "message": "current_post implies callee_pre: failed"
}
```

## Callsite resolution index format (per #312)

The seed catalog provides a callsite index mapping:

- Key: `file:line:character` (canonical callsite identifier)
- Value: `{ callee: "path/to/function", pre: Post, post: Post }`

Example: `Array.prototype.push`, `String.prototype.startsWith` (globalThis prefix format for JS/TS).