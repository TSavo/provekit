# Realize-kit synthesis-from-op-tree design

Date: 2026-05-18
Architect: Sir (T Savo)
Coordinator: Kit
Status: design — codex briefs derive from this

## Problem

Trinity slow-lane fixture_01 (`compute_sum(a, b)`) panics at `trinity_citation_comments_exhibit.rs:562` with `lower to java must use real body templates, not a stub`. The function is a user-defined composition (`a + b`, then `total * 2`, then `scaled - 1`) — not a single cataloged concept. The realize kit's per-function-concept template lookup misses and falls back to a stub.

PR #1205 made the bind kit emit Shape A op trees: the bind payload now contains `Term::Op { name: "concept:add", ... }` walkable nodes nested inside the bind-result tree. The realize kits need to walk that tree and synthesize per-language source.

## Non-goals

- Not adding new template engines. Reuse existing `renderBodyTemplateFor` (Java), `operator_body_template_for` + `render_template` (Rust), `body_template_for` (Python).
- Not changing the body-template-memento contract or `*-canonical-bodies.json` schema beyond adding entries.
- Not minting new substrate concepts. Operators like `concept:add` are already cataloged with `op_def`.

## Design

### 1. Body-template entries (data, all three languages)

Add per-operation template entries to:
- `menagerie/java-language-signature/specs/body-templates/java-canonical-bodies.json`
- `menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json`
- `menagerie/python-language-signature/specs/body-templates/python-canonical-bodies.json`

Operator set (per inventory from `mint_language_morphisms.py` and Trinity lifters):

| Concept | Java template | Rust template | Python template |
|---|---|---|---|
| `concept:add` | `${operand_0} + ${operand_1}` | `${operand_0} + ${operand_1}` | `${operand_0} + ${operand_1}` |
| `concept:sub` | `${operand_0} - ${operand_1}` | `${operand_0} - ${operand_1}` | `${operand_0} - ${operand_1}` |
| `concept:mul` | `${operand_0} * ${operand_1}` | `${operand_0} * ${operand_1}` | `${operand_0} * ${operand_1}` |
| `concept:div` | `${operand_0} / ${operand_1}` | `${operand_0} / ${operand_1}` | `${operand_0} // ${operand_1}` |
| `concept:mod` | `${operand_0} % ${operand_1}` | `${operand_0} % ${operand_1}` | `${operand_0} % ${operand_1}` |
| `concept:eq` | `${operand_0} == ${operand_1}` | `${operand_0} == ${operand_1}` | `${operand_0} == ${operand_1}` |
| `concept:ne` | `${operand_0} != ${operand_1}` | `${operand_0} != ${operand_1}` | `${operand_0} != ${operand_1}` |
| `concept:lt` | `${operand_0} < ${operand_1}` | `${operand_0} < ${operand_1}` | `${operand_0} < ${operand_1}` |
| `concept:le` | `${operand_0} <= ${operand_1}` | `${operand_0} <= ${operand_1}` | `${operand_0} <= ${operand_1}` |
| `concept:gt` | `${operand_0} > ${operand_1}` | `${operand_0} > ${operand_1}` | `${operand_0} > ${operand_1}` |
| `concept:ge` | `${operand_0} >= ${operand_1}` | `${operand_0} >= ${operand_1}` | `${operand_0} >= ${operand_1}` |
| `concept:not` | `!${operand_0}` | `!${operand_0}` | `not ${operand_0}` |
| `concept:neg` | `-${operand_0}` | `-${operand_0}` | `-${operand_0}` |
| `concept:bitand` | `${operand_0} & ${operand_1}` | `${operand_0} & ${operand_1}` | `${operand_0} & ${operand_1}` |
| `concept:bitor` | `${operand_0} \| ${operand_1}` | `${operand_0} \| ${operand_1}` | `${operand_0} \| ${operand_1}` |
| `concept:bitxor` | `${operand_0} ^ ${operand_1}` | `${operand_0} ^ ${operand_1}` | `${operand_0} ^ ${operand_1}` |
| `concept:bitnot` | `~${operand_0}` | `!${operand_0}` | `~${operand_0}` |
| `concept:shl` | `${operand_0} << ${operand_1}` | `${operand_0} << ${operand_1}` | `${operand_0} << ${operand_1}` |
| `concept:shr` | `${operand_0} >> ${operand_1}` | `${operand_0} >> ${operand_1}` | `${operand_0} >> ${operand_1}` |
| `concept:ite` | `${operand_0} ? ${operand_1} : ${operand_2}` | `if ${operand_0} { ${operand_1} } else { ${operand_2} }` | `${operand_1} if ${operand_0} else ${operand_2}` |
| `concept:assign` | `${operand_0} = ${operand_1}` | `${operand_0} = ${operand_1}` | `${operand_0} = ${operand_1}` |
| `concept:seq` | `${operand_0};\n${operand_1};\n...` | (newline-joined) | (newline-joined) |
| `concept:return` | `return ${operand_0}` | `return ${operand_0}` | `return ${operand_0}` |

Entry schema (per existing format):
```json
{
  "concept_name": "concept:add",
  "mode": null,
  "min_params": 2,
  "max_params": 2,
  "template": "${operand_0} + ${operand_1}",
  "loss_record": {}
}
```

### 2. Recursion wiring

Per-realize-kit: extend the lookup path so when `concept_name` matches a cataloged operator (i.e., a `concept:add`-style entry rather than a function-level concept), the renderer:

1. Looks up the operation template by `concept_name`.
2. For each operand position, resolves the operand from the bind payload's op-tree args:
   - If arg is a `Term::Op` with `concept_name` starting with `concept:` → recursively render via this same path.
   - If arg is a `Term::Const` with a literal value (number/bool/string) → emit the literal in language-appropriate form.
   - If arg is a `Term::Var` or operand-binding ref → emit the variable name.
3. Substitutes resolved operands into `${operand_0}`, `${operand_1}`, etc. placeholders.
4. Returns the rendered string for composition into the parent.

The TOP-LEVEL function-concept (`UNNAMED-CONCEPT-1` for `compute_sum`) needs a fallback: when no function-concept template matches, treat the named_term_tree as a `concept:seq` of statements and synthesize the body by recursing into each statement.

### 3. Operand types and resolution

Per existing bind payload (from `named_tree_op_tree` in `bind.rs:996`):
- `Term::Op { name, op_cid, args }` — nested operation. `args[0]` is citation metadata (Term::Const); `args[1..]` are operand sub-terms.
- `Term::Const { value, sort }` — literal. Render `value` as JSON in target language (true/false; integer; string).
- `Term::Var { name }` — variable reference. Render as `name` (after sanitization).

The first arg of every operation `Term::Op` is a citation metadata Term::Const — skip it when walking operands.

### 4. Test acceptance

For each language, fixture_01 (`compute_sum`) must produce non-stub output that:
- Compiles (Java, Rust) or runs (Python).
- When invoked as `compute_sum(3, 4)`, returns 13.

CI gates: trinity_citation_comments_exhibit slow-lane goes green for fixture_01.

## Per-language codex briefs

Three parallel codex (gpt-5.5 xhigh) tasks in isolated worktrees. Each brief is self-contained.

### Java brief

Files:
- `implementations/java/provekit-realize-java-core/src/main/java/com/provekit/realize/SugarRealizer.java` — add per-operation template recursion at `renderBodyTemplateFor` or sibling
- `menagerie/java-language-signature/specs/body-templates/java-canonical-bodies.json` — add entries from operator table above

Tests:
- Existing `BodyTemplateWiringTest.java` — extend with multi-op synthesis cases
- `cargo test -p provekit-cli --test trinity_citation_comments_exhibit --features slow-tests` — slow-lane gate (after Rust+Python also done)

### Rust brief

Files:
- `implementations/rust/provekit-realize-rust-core/src/lib.rs` — extend `lower_term_shape_body` to descend non-seq operation trees; integrate `operator_body_template_for` recursion
- `menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json` — add entries

Tests:
- Existing unit tests in `lib.rs` (e.g., `lowers_assert_statement_macro_to_opaque_macro_call`) — extend with synthesis cases

### Python brief

Files:
- `implementations/python/provekit-realize-python-core/src/provekit_realize_python_core/realizer.py` — add tree-walking and template recursion
- `menagerie/python-language-signature/specs/body-templates/python-canonical-bodies.json` — add entries

Tests:
- Existing `test_realizer.py` — extend

## Constraints

- No em-dashes or en-dashes anywhere (substrate rule).
- Commit identity: `T Savo <evilgenius@nefariousplan.com>`.
- Codex model: `gpt-5.5` with `model_reasoning_effort=xhigh` (substrate default).
- Codex has no `gh` write access; Kit handles all gh ops.
- Isolation: each codex runs in a separate `git worktree`.
- After each codex completes, Kit verifies + commits if needed.

## Sequence

1. Codex A: Java synthesis (this file's Java brief).
2. Codex B: Rust synthesis (this file's Rust brief).
3. Codex C: Python synthesis (this file's Python brief).
4. Kit: integration verification — run `trinity_citation_comments_exhibit --features slow-tests` after all three land.
5. Kit: mint per-language `eq_and_to_ite_desugar`, `eq_or_to_ite_desugar` mementos (substrate hygiene; out of critical path).

Parallel-vs-sequential: per agent isolation, dispatch all three in parallel under separate worktrees. Kit integrates results.
