# The Program Is The Proof

Date: 2026-05-10

This is the worked proof for:

```c
static int foo(int x) { if (x == 0) return -22; return x; }
```

The claim is operationally true on this fixture: the libclang AST can be materialized as the C11 algebra term

```text
seq(if(eq(x, 0), return(neg(22)), skip), return(x))
```

and `project(term)` is byte-identical, after JSON parsing/JCS-equivalent object comparison, to the current `collectors-defensive` `function-contract` for `foo`.

## Artifacts

- Source: `menagerie/c11-language-signature/example/foo.c`
- Target hand-authored term: `menagerie/c11-language-signature/example/foo.term.json`
- Generated term: `menagerie/c11-language-signature/example/foo.generated.term.json`
- Current collectors contract: `menagerie/c11-language-signature/example/foo.contract.json`
- Generated projected contract: `menagerie/c11-language-signature/example/foo.projected-contract.json`
- Add source: `menagerie/c11-language-signature/example/add.c`
- Add generated term: `menagerie/c11-language-signature/example/add.term.json`
- Add generated projected contract: `menagerie/c11-language-signature/example/add.projected-contract.json`
- Implementation: `implementations/c/provekit-walk-c/src/term_project_main.c`
- Test: `implementations/c/provekit-walk-c/tests/term_project.sh`

## Existing Lifters

Build commands run:

```sh
make -C implementations/c/provekit-lift-c-collectors-defensive
make -C implementations/c/provekit-walk-c
```

The `collectors-defensive` JSON-RPC lift of `foo.c` emits a `function-effects` declaration and a `function-contract` declaration. The `function-contract` declaration is the same object as `foo.contract.json`: `pre = true`, `post = result = ite(x == 0, -22, x)`, no effects, `i32 -> i32`.

The `provekit-walk-c` JSON-RPC lift of standalone `foo.c` currently emits only `function-effects`; that matches `foo.walk-c-rpc.jsonl`. This is not a contradiction in the proof: `walk-c`'s WP-chain emitter is callsite-driven. With no caller/callee precondition chain in `foo.c`, it has no chain declaration to emit. The relevant WP machinery is still in `walk-c`: it collects body statements, walks backward from a callsite, applies declarations/conditionals/guards, records arrivals, and serializes the chain.

## The AST Term

The libclang cursor path for `foo` is:

```text
FunctionDecl foo
  CompoundStmt
    IfStmt
      BinaryOperator "x == 0"
        DeclRefExpr x
        IntegerLiteral 0
      ReturnStmt
        UnaryOperator "-"
          IntegerLiteral 22
      <missing else> => skip
    ReturnStmt
      DeclRefExpr x
```

The C11 operation CIDs used by this term are read from `menagerie/c11-language-signature/component-cids.json`:

| op | cid |
| --- | --- |
| `seq` | `blake3-512:f8390f57e0f4408252211849b4e62639c9779a19bcdfc207eb80e2f3225e2f3a1262434a0e56b0de765b35ad377ffee3b91e69750996104505dc9ed7c1398915` |
| `if` | `blake3-512:402feb91b68096553e0c7f000cdb47c50a2d16094571a426639d6be487b934b9f1664bd4b8aa5f8cc2acf5d47f44cee31882c6dc8799e71aa29381fd91309b65` |
| `eq` | `blake3-512:be234846ff5993e9492eedb48c537f9750a5992e882e90b640728b52ad91b3d94c9b67e9f6b3f140711feb149e273a089eed6fb8eb337ab2dea6b2b11b0cfa7b` |
| `return` | `blake3-512:5f1b6815fc786463b21234a14b2216a5156ebd5e385eadb1c749c0fa62e28e09f38adc37de9da36502200a4ee8e364e08b23f8767267e04dfb700c3061bb0428` |
| `neg` | `blake3-512:9c272325e9a7d7e9d8c5f7b575f36b9024ffd45f8c4863343be39aa6c6573548898b3c6ffb8566b1d44f51bead519871af6acb3fa5ad0f1dd46b7aa04ac30961` |
| `skip` | `blake3-512:f6d5647365eb408ec445a22218b0587f23c37a6a635fac4f398a48469fb7a190c7751bac9640b918b0f27f08038ad7dc38837cbd718115bb99644cc5a7fbb93a` |

The emitted term memento uses the existing fixture schema: op nodes carry `name`, not inline `op_cid`; the signature CID identifies the operation namespace.

## Implementation

The proof harness is additive and deliberately does not modify `collectors-defensive`, because another worktree is changing that lifter. The new tool is `provekit-c11-term-project`.

Important implementation points:

- Parse source with libclang using the same C cursor surface as `walk-c`: `term_project_main.c:694`.
- Fail closed with an unsupported cursor-kind diagnostic rather than emitting lossy algebra: `term_project_main.c:386`.
- Convert expressions to C11 op nodes: binary operators map to `eq`, `add`, `sub`, `mul`, etc. at `term_project_main.c:421`, and expression lifting is at `term_project_main.c:504`.
- Convert statements to C11 statement terms: sequence folding is at `term_project_main.c:543`, `return` at `term_project_main.c:581`, `if` at `term_project_main.c:607`, and statement dispatch at `term_project_main.c:637`.
- Emit the `c11-algebra-term` memento at `term_project_main.c:783`.
- Implement `project` over the materialized term at `term_project_main.c:816`.
- Convert the projected value back to the current `function-contract` shape at `term_project_main.c:859` and `term_project_main.c:926`.

The implementation uses the existing `walk-c` expression helpers where useful (`pk_c_walk_cursor_source`, `pk_c_walk_lift_formals`, locus helpers), but it introduces a separate C11 statement term because `pk_c_walk_term` in `walk_c.h` is currently an expression/formula term type, not a full statement-flow term.

## Projection

For this demonstrator, `project` computes the returned value under a continuation, equivalent to the branch-sensitive WP projection for these straight-line/conditional returns:

```text
project_value(return(e), k) = e
project_value(skip, k) = k
project_value(seq(a, b), k) = project_value(a, project_value(b, k))
project_value(if(c, t, e), k) = ite(c, project_value(t, k), project_value(e, k))
```

Then the function contract is:

```text
pre  = true
post = result = project_value(term, bottom)
```

For `foo`:

```text
project_value(return(x), bottom) = x
project_value(skip, x) = x
project_value(return(neg(22)), x) = -22
project_value(if(eq(x, 0), return(neg(22)), skip), x)
  = ite(eq(x, 0), -22, x)
project_value(seq(if(...), return(x)), bottom)
  = ite(eq(x, 0), -22, x)
```

The generated projected contract is exactly:

```text
pre  = true
post = result = ite(x == 0, -22, x)
```

Verification commands:

```sh
python3 -c 'import json; a=json.load(open("menagerie/c11-language-signature/example/foo.term.json")); b=json.load(open("menagerie/c11-language-signature/example/foo.generated.term.json")); print(a==b)'
python3 -c 'import json; a=json.load(open("menagerie/c11-language-signature/example/foo.contract.json")); b=json.load(open("menagerie/c11-language-signature/example/foo.projected-contract.json")); print(next(x for x in a if x.get("kind")=="function-contract" and x.get("fn_name")=="foo")==b)'
```

Both print `True`. `foo.expected-wp-contract.json` also matches after dropping its explanatory `source_term` field.

## Add Example

For:

```c
int add(int a, int b) { return a + b; }
```

the generated term surface is:

```text
return(add(a, b))
```

and the projected contract is:

```text
pre  = true
post = result = +(a, b)
```

This is deliberately smaller than `foo`; it proves the same path for a single `ReturnStmt` and `BinaryOperator`.

## The Gap

One-sentence statement:

> The AST visitors already compute contract/WP projections from control-flow structure, but they accumulate `pre`/`post` or WP formulas directly; no first-class `ITerm` for the C11 statement algebra is threaded through and then projected.

Precise locations:

- `provekit-walk-c/src/walk.c:140` collects `FunctionDecl` metadata and immediately computes `fn.pre` through `pk_c_walk_lift_function_pre`; no full body term is stored.
- `provekit-walk-c/src/walk.c:178` finds the `CompoundStmt`; `walk.c:199` collects body statements as cursors, not as a term.
- `provekit-walk-c/src/lift.c:146` lifts an expression cursor by re-parsing source text into a `pk_c_walk_term`; this is expression-level, not a statement-flow term.
- `provekit-walk-c/src/lift.c:223` computes an if-exit precondition directly, and `lift.c:250` accumulates function preconditions directly.
- `provekit-walk-c/src/walk.c:373` walks prior statements backward, applying declaration, conditional, and guard projections directly to `wp`.
- `provekit-walk-c/src/conditional.c:362` substitutes branch assignments into `wp`, `conditional.c:376` and `conditional.c:382` build branch implications, and `conditional.c:388` conjoins them. That is `project(if(...))`, but the `if` term is never materialized.
- `provekit-walk-c/src/contract.c:166` serializes the WP chain; `contract.c:224` writes `post` from the first arrival's WP and `contract.c:226` writes `pre` from the final arrival's WP.
- `collectors-defensive/src/patterns.c:1759` scans if/return structure, `patterns.c:1816` builds the branch-sensitive `ite`, `patterns.c:1907` scans the sequence, and `patterns.c:1980` sets `post` directly.
- `collectors-defensive/src/walker.c:326` calls type/defensive extractors into a mutable contract accumulator, and `walker.c:341` emits the direct contract object.

That is the exact gap: the proof term is implicit in the traversal and discarded.

## Proof Reading For `foo`

Read `foo.c` as the proof term:

```text
T = seq(I, R_x)
I = if(C, R_neg22, skip)
C = eq(x, 0)
R_neg22 = return(neg(22))
R_x = return(x)
```

The local proof links are:

```text
R_x establishes result = x
skip followed by R_x establishes result = x
R_neg22 establishes result = -22
C -> R_neg22 establishes C -> result = -22
not(C) -> skip; R_x establishes not(C) -> result = x
I followed by R_x establishes result = ite(C, -22, x)
T establishes result = ite(C, -22, x)
true is the required entry precondition
```

Those implications hold by the C11 operation rules for `return`, `skip`, `if`, and `seq`. There is no search over the program: checking this proof is a walk over the term, verifying the op CID at each node and replaying the local projection rule. For this program the proof size is constant in practice: seven operation nodes (`seq`, `if`, `eq`, `return`, `neg`, `skip`, `return`) plus the contract decoration.

This is the path-map reading intended for paper 17's §7: `parse` gives the C11 term, `project` gives the contract/WP/effect views, and `check` walks the same flow and verifies the adjacent implications. The eight-primitives framing does not need another primitive for this; the missing move is to stop throwing away the intermediate term between `parse` and each projection.

## Right-Way Refactor Plan

The right refactor for `collectors-defensive` is not to bolt on another parallel emitter. It is:

1. Introduce a C11 `ITerm` module shared by the C lifters.
   - New files: `implementations/c/provekit-lift-core/include/provekit/c11_term.h`, `implementations/c/provekit-lift-core/src/c11_term.c`.
   - Include statement ops (`seq`, `if`, `return`, `skip`, assignment/control/effect ops) and expression ops (`eq`, `add`, `neg`, etc.).
   - Keep `provekit-walk-c/src/walk_c.h:13` in mind: `pk_c_walk_term` exists, but it is expression/formula-shaped and not enough for statement flow.

2. Move the libclang term builder out of the proof harness.
   - Source template: `implementations/c/provekit-walk-c/src/term_project_main.c:421-653`.
   - Production target: `implementations/c/provekit-lift-core/src/c11_term_clang.c`.
   - It must keep the fail-closed behavior from `term_project_main.c:386`.

3. Replace direct branch-return post synthesis in `collectors-defensive`.
   - Current direct path: `patterns.c:1759-1869` builds branch-sensitive return terms, `patterns.c:1907-1960` scans sequence returns, and `patterns.c:1980` writes `contract->post`.
   - Replacement: build `ITerm` once for the function body, call `project_contract(ITerm)`, and populate `contract->post` from that projection.

4. Keep existing precondition/effect extraction additive until they are projected too.
   - Current pre/effect accumulation is in `patterns.c:1526-1622`, type predicates in `types.c`, and effect serialization in `walker.c:251-255`.
   - The first safe refactor preserves those outputs byte-for-byte and swaps only the return-post source from direct scanner to `project(ITerm)`.

5. Replace `parse -> contract` in the walker with `parse -> ITerm -> project -> contract`.
   - Current loop target: `collectors-defensive/src/walker.c:302-349`.
   - Current serializer target: `walker.c:188-255`.
   - Keep the serializer stable so existing tests continue to compare the same JSON shape.

6. Then unify projections.
   - Contract projection uses the new `project_contract(ITerm)`.
   - WP-chain projection reuses/refactors the rules now visible in `walk-c/src/conditional.c:362-388` and `walk-c/src/contract.c:166-227`.
   - Effects projection eventually uses the same `ITerm` and the existing effects extractor as a cross-check.

This worktree did the demonstration, not the collectors refactor, to avoid colliding with the separate bolted-on `c11-algebra-term` emit work. The proof-of-concept is enough to establish that the contract already is `project(term)` for `foo`.

## Verification State

The focused test is:

```sh
sh implementations/c/provekit-walk-c/tests/term_project.sh
```

It checks:

- generated `foo` term equals `foo.term.json`;
- generated `project(foo term)` equals the `function-contract` inside `foo.contract.json`;
- generated `add` term has `term_surface == "return(add(a, b))"`;
- generated `project(add term)` has `post = result = +(a, b)`.

The full `provekit-walk-c` `make test` now includes this test.
