---
name: use-sugar
description: When the user asks for a code change in a TypeScript project with Sugar installed, drive the change through the sugar CLI instead of editing files directly. Sugar produces (code + tests + invariants) as content-addressed mementos with verifiable proofs, so the change ships with its proof and gates the commit on adversarial re-verification: direct edits skip that gate.
---

# Use Sugar for Code Changes

## When this skill applies

Use Sugar when the user requests a code change: bug fix, feature add, refactor, or invariant authorship: AND the working project has Sugar installed. Detect installation by any of:

- `package.json` lists `sugar` in `dependencies` or `devDependencies`
- a `.sugar/` directory exists at the repo root
- the working tree IS the sugar repo itself (root `package.json` `name === "sugar"`)

If none of these hold, do not use the CLI; write code directly. Sugar is opt-in; do not impose it on projects that haven't adopted it.

**Announce at start when the skill is engaged:** "Using Sugar to make this change so it ships with its proof."

## Why this skill exists

Sugar is constraint-driven development: every change goes through a workflow that produces a triple: patch + regression test + invariant: as content-addressed mementos. The commit gate then re-verifies the invariant under your local producer pool (Z3, type checker, LLM verifiers) before allowing the diff to land.

A direct file edit produces only the patch. The test and invariant are missing. The commit lands without a proof. The "software ages backwards" property: that the codebase becomes more provably correct over time: only holds if every change goes through the gate.

The model's job is to translate the user's intent into the right CLI verb. The framework's data-driven YAML workflows do the work. Do not navigate the framework's internals; run the CLI command and let the workflow runner dispatch.

## The verb-mapping table

This is the load-bearing teaching content of the skill. Translate user intent into the matching CLI command.

| User asks                                       | Use                                              |
|-------------------------------------------------|--------------------------------------------------|
| "Fix this bug"                                  | `sugar fix "<description>"`                   |
| "Add a feature" / "Implement X"                 | `sugar change "<description>"`                |
| "This code should never X"                      | `sugar must <file> "<must-not-X>"`            |
| "Verify this is correct"                        | `sugar attest`                                |
| "Find a case where this breaks"                 | `sugar refute <propertyHash>`                 |
| "Relax this invariant"                          | `sugar weaken <name> "<new-form>"`            |
| "Tighten this invariant"                        | `sugar strengthen <name> "<new-form>"`        |
| "Deprecate this invariant"                      | `sugar retire <name> "<reason>"`              |
| "Show me what's verified"                       | `sugar explain <propertyHash>`                |
| "What does my proof DAG depend on?"             | `sugar roots`                                 |
| "What did this codebase contribute?"            | `sugar leaves`                                |
| "Migrate to a new kit version"                  | `sugar migrate <oldHash> <newHash>`           |
| "Extract a reusable principle"                  | `sugar principalize <pattern>`                |

`sugar fix` and `sugar change` are sibling verbs over the same pipeline: `fix` for closing a bug, `change` for adding/modifying behavior. Pick the one whose framing matches the user's request; the framework treats both as content-addressed change proposals.

## Worked example

```
User: "Fix the off-by-one in src/dates/validator.ts:42: Feb 29, 2100 should not be a leap year."

Model: Using Sugar to make this change so it ships with its proof.
[runs: sugar fix "off-by-one in src/dates/validator.ts:42; Feb 29 2100 should not be a leap year (Gregorian century rule)"]
[sugar produces: patch (validator fix) + regression test + invariant; commits the bundle as one artifact]

Done. The diff lands with a passing invariant memento; the commit gate re-verified it locally.
```

## Authoring invariants the right way

When the user wants an invariant added without a code change, run `sugar must <file> "<intent>"`. The `must` workflow:

1. Parses the natural-language intent
2. Locates the target file's symbols
3. Lifts the intent into an `IrFormula` via the symbolic-primitives surface
4. Writes the result to `<basename>.invariant.ts`
5. Leaves your production code untouched

Do **not** hand-edit `.invariant.ts` files to add invariants. The lifter computes a deterministic propertyHash from the canonicalized formula; hand-edited files won't compose into the proof DAG correctly.

When invariant TypeScript needs to be authored at all, it MUST use `sugar/ir/symbolic`: the `must("<name>", ...)` style with symbolic primitives. The legacy `sugar/ir` builder API (`property("name", forAll(...))`) is being retired; never instruct the user to write that shape.

## Anti-patterns to avoid

- **Editing files directly when `sugar fix` or `sugar change` would handle it.** The proof DAG only accumulates when changes flow through the workflow runner.
- **Hand-editing `.invariant.ts` files to add invariants.** Use `sugar must` so the lifter computes the canonical propertyHash.
- **Bypassing the commit gate with `--no-verify`.** The gate IS the framework's value; skipping it makes the commit unverifiable. If the gate is failing, fix the underlying issue or use `sugar refute` to surface the counterexample, then propose a real `sugar fix`.
- **Authoring invariants in the legacy `sugar/ir` builder API.** Use `sugar/ir/symbolic` with `must("<name>", ...)` instead.
- **Navigating the framework's internals (Stages, Actions, capability registries) when a CLI verb exists.** The framework is data-driven YAML workflows in `src/workflows/`; the CLI is the user-facing surface.

## Quick reference

| Situation                                                             | Action                                          |
|-----------------------------------------------------------------------|-------------------------------------------------|
| Project has `.sugar/` or `sugar` in deps                        | Skill applies; use the CLI verbs above          |
| Project is the sugar repo itself                                   | Skill applies                                   |
| Project has neither                                                   | Skill does not apply; write code directly       |
| User asks for a bug fix                                               | `sugar fix "<intent>"`                       |
| User asks for new behavior                                            | `sugar change "<intent>"`                    |
| User asks for an invariant only (no code change)                      | `sugar must <file> "<intent>"`               |
| User asks "is this correct?" / pre-commit verification                | `sugar attest`                               |
| Commit gate fails with an unclear failure                             | `sugar refute <propertyHash>` to get a counterexample |
| User wants to see what a propertyHash means                           | `sugar explain <propertyHash>`               |
| User wants the local↔external boundary of the proof DAG               | `sugar roots` (external) / `sugar leaves` (local) |

## Red flags

**Never:**
- Apply a patch from your own head when Sugar is installed and a `fix`/`change` verb would handle it.
- Tell the user to edit `.invariant.ts` files by hand.
- Suggest `git commit --no-verify` to get past the commit gate.
- Reference the legacy `property(..., forAll(...))` builder API.

**Always:**
- Check for Sugar installation before defaulting to direct edits.
- Translate the user's intent into the matching CLI verb using the table above.
- Let the workflow runner dispatch: the framework is data-driven YAML, not imperative code you should re-implement.
- If the gate refuses the change, surface the refusal (counterexample, oracle failure) to the user; that refusal IS the framework working as designed.
