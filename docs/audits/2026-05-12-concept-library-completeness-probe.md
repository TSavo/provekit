# Concept-Library Completeness Probe

**Audit date**: 2026-05-12
**Baseline commit**: `21cd6981` (`docs(readme): add federation-by-construction section (#741)`)
**Baseline CID anchor**: `cids.tsv` sha256 `cf3ca975880213949e3a621a33c607fb645369b2b1243c82c689623f1f1512b2` (437 rows)
**Scope**: cross-language idiom completeness -- what concept-hub ops are MISSING for idioms common
across the 10 actively-minted languages. This is the complement to the same-day operation-layer
probe (`2026-05-12-concept-library-completeness-probe-operation-layer.md`), which covers
morphism discharge gaps for existing ops on {c11, java, python}. This audit asks which
_new concept-hub nodes_ are needed for idioms the hub does not name at all.
**Boundary**: 1.7.0 ships with the kinds and concepts it has. New concept ops queue for 1.8.0.

---

## 1. Baseline

### 1.1 Concept-Hub Primitive Op Nodes (45)

These are the `*_shape.spec.json` files that are _not_ abstraction-tier pattern shapes:

```
add, addr, assign, bitand, bitnot, bitor, bitxor, break, call, cast,
conditional, continue, decl, deref, div, do, eq, for, ge, gt, index,
ite, le, lt, member, mod, mul, ne, neg, new, not, postdec, postinc,
predec, preinc, return, seq, shl, shr, skip, source-unit, sub, throw,
ushr, while
```

### 1.2 Abstraction-Tier Pattern Shapes (7)

Not primitive op nodes; zero op-layer morphism coverage is correct for these:

```
acquire-use-release, allocate-or-bail, branch-on-error-else-passthrough,
check-bounds-then-access, refcount-inc-use-dec, validate-then-commit,
validated-allocated-access
```

**Total `*_shape.spec.json` files: 52** (45 primitive op + 7 pattern shapes)

### 1.3 Actively-Minted Languages (10)

The 10 languages for which `scripts/mint_language_morphisms.py` generates morphisms and
transport-gap rows:

```
c11, csharp, go, java, php, python, ruby, rust, typescript, zig
```

Excluded from the "3+ languages" threshold:
- `aarch64` -- assembly tier, not a source language
- `cpp` -- directory exists, lifter not yet generating morphisms
- `evm-bytecode`, `jvm-bytecode` -- bytecode tiers
- `swift` -- directory exists, lifter not yet generating morphisms

### 1.4 Demoted or Deliberately Out-of-Hub Idioms

Recorded in `transport-gaps.md` Semantic Restrictions. These are NOT missing concepts --
they are explicitly architectural decisions. They appear in Section 4, not Section 2.

- `concept:foreach` -- demoted; no common iterator protocol across 10 languages
- `concept:and`, `concept:or` -- demoted; McCarthy desugar to `concept:ite`
- `concept:floordiv` -- python-specific; not structurally portable

---

## 2. Missing Concepts

Idioms present in 3+ actively-minted languages with no corresponding concept-hub op and no
existing demotion memento or transport-gap note.

### 2.1 Missing-Concepts Table

| Proposed op | Languages where idiom appears | Closest existing concept | Priority | Suggested operator + arity + effect |
| --- | --- | --- | --- | --- |
| `concept:assert` | c11 (`assert.h`), java (`assert`), python (`assert`), rust (`assert!`/`debug_assert!`), go (`t.Fatal` / no `assert` keyword), cpp (`assert`), zig (`std.debug.assert`) | `concept:throw` (wrong: assert is conditional abort, not user-thrown exception) | P1 | `fn_name: concept:assert, formals: [cond: Bool], return: Stmt, effects: [{kind: effect-signature, name: Panic}]` |
| `concept:switch` | c11 (`switch`), java (`switch`), go (`switch`), csharp (`switch`), typescript (`switch`), php (`switch`), rust (`match`), zig (`switch`) | `concept:conditional` (wrong: multi-way branch, not binary) | P1 | `fn_name: concept:switch, formals: [scrutinee: Expr, arms: ListOfArm], return: Stmt, effects: [{kind: effect-polymorphic, rule: union(arms.effects)}]` |
| `concept:try-catch` | java (`try/catch`), typescript (`try/catch`), php (`try/catch`), python (`try/except`), ruby (`begin/rescue`), csharp (`try/catch`) | `concept:throw` (complementary: throw is raise-side; try-catch is handle-side) | P1 | `fn_name: concept:try, formals: [body: Stmt, handler: Stmt], return: Stmt, effects: [{kind: effect-polymorphic, rule: body.effects minus caught}]` |
| `concept:null-literal` | c11 (`NULL`), java (`null`), python (`None`), typescript (`null/undefined`), php (`null`), ruby (`nil`), go (`nil`), csharp (`null`), rust (absent; Option carries this), zig (`null` in optionals) | none | P1 | `fn_name: concept:null, formals: [], return: Expr, effects: []` -- note: absence in Rust is architecturally significant; the 1.8.0 spec should call it out |
| `concept:int-literal` | all 10 languages | none (existing `concept:source-unit` wraps bytes, not literal nodes) | P2 | `fn_name: concept:int-literal, formals: [value: Int], return: Expr, effects: []` -- companion to `concept:bool-literal` if one is minted |
| `concept:bool-literal` | all 10 languages (true/false or 1/0) | none | P2 | `fn_name: concept:bool-literal, formals: [value: Bool], return: Expr, effects: []` |
| `concept:string-literal` | all 10 languages | none | P2 | `fn_name: concept:string-literal, formals: [value: String], return: Expr, effects: []` |
| `concept:goto` | c11 (`goto`), php (`goto`), go (no `goto` but has labeled `break`), zig (`break :label`) | none (c11:goto is `unmapped [no-concept-target]` per the operation-layer audit) | P2 | `fn_name: concept:goto, formals: [label: String], return: Stmt, effects: [{kind: effect-signature, name: Goto}]` -- architectural note: semantically `goto` is not structurally composable; 1.8.0 spec should declare scope as c11+php only |
| `concept:compound-assign` | c11 (`+=`, `-=`, etc.), java, typescript, go, php, python, ruby, csharp, rust | none (c11 compound-assign ops are all `unmapped [no-concept-target]`) | P2 | `fn_name: concept:compound-assign, formals: [target: LValue, op: String, value: Expr], return: Stmt, effects: [{kind: effect-signature, name: Mutate}]` -- unifies `+=/-=/*=/etc.` as a parameterized form; avoids 10 new specs |
| `concept:lambda` | java (lambdas), typescript (arrow functions), python (lambda), ruby (Proc/lambda), go (func literals), rust (closures), csharp (lambdas), php (anonymous functions) | `concept:call` (wrong: call is invocation; lambda is construction of a callable) | P3 | Needs architectural decision: captures + effect set of the body are not statically knowable at construction site; effect signature is open |
| `concept:spawn` | go (`goroutine`), rust (`thread::spawn`), java (`Thread`, `CompletableFuture`), typescript (`Promise`, `Worker`), python (`Thread`, `asyncio.create_task`), csharp (`Task`) | none | P3 | Requires concurrency execution model; effect signature depends on whether substrate tracks inter-thread causality |
| `concept:channel-send` | go (channels), rust (mpsc, tokio), typescript (async patterns), java (BlockingQueue) | none | P3 | Requires channel type in sort algebra; channel-send and channel-recv are dual primitives |
| `concept:channel-recv` | go (channels), rust (mpsc, tokio), typescript, java | none | P3 | Dual of channel-send |
| `concept:await` | typescript (`await`), python (`await`), rust (`.await`), java (`CompletableFuture.get`), csharp (`await`) | none | P3 | Async execution model: `await` suspends current continuation; effect set needs coroutine/async-context tag |
| `concept:async-fn` | typescript (`async function`), python (`async def`), rust (`async fn`), java (async via virtual threads), csharp (`async`) | none | P3 | Marks function as returning a future; interacts with `concept:await`; needs paired-primitive spec |
| `concept:alloc` | c11 (`malloc`), cpp (`new`), rust (`Box::new`, allocators), go (implicit via `make`/`new`), zig (`allocator.alloc`) | `concept:new` (partial: `new` is construction, `alloc` is raw heap reservation) | P3 | `alloc` and `new` are already differentiated in the effect system (`Alloc` effect); needs a raw-allocation node separate from typed construction |
| `concept:free` | c11 (`free`), cpp (`delete`), zig (`allocator.free`) | none | P3 | GC languages don't expose free; scope is manual-memory-management languages only; needs language-set annotation |
| `concept:move` | rust (`move` semantics), cpp (`std::move`), zig (move by default) | none | P3 | Ownership transfer; architecturally distinct from copy/clone; effect system needs an ownership-transfer tag |
| `concept:borrow` | rust (`&`, `&mut`), cpp (references) | `concept:addr` (wrong: borrow is a lifetime-scoped alias, addr is a raw pointer) | P3 | Borrow vs addr is a fundamental semantic split; minting requires a lifetime sort in the algebra |
| `concept:iter` | all 10 languages (for-each iteration protocol) | none (foreach was demoted because iter protocol was missing) | P3 | `concept:iter`, `concept:has-next`, `concept:next` as a triple; prerequisite for undemotion of `concept:foreach` |
| `concept:map-op` | all 10 languages (`map`, `Select`, `fmap`) | none | P3 | Higher-order; formal sort includes a function type; function sort not yet in the algebra |
| `concept:filter-op` | all 10 languages | none | P3 | Same: higher-order; needs function sort |
| `concept:fold-op` | all 10 languages (`reduce`, `fold`, `aggregate`) | none | P3 | Same: higher-order |
| `concept:pos` | python (`+x` unary plus), java (`+x`), typescript (`+x`) | none (per operation-layer audit R2, `python:pos` has no hub analog) | P2 | `fn_name: concept:pos, formals: [operand: Expr], return: Expr, effects: []` -- near-noop but structurally required to discharge python:pos |
| `concept:pow` | python (`**`), ruby (`**`), php (`**`), typescript (`**`), java (none, `Math.pow`), go (none, `math.Pow`) | none (per operation-layer audit R2) | P2 | `fn_name: concept:pow, formals: [base: Expr, exponent: Expr], return: Expr, effects: []` -- present as operator in python/ruby/php/typescript; absent as operator in java/go |

---

## 3. Language-Signature Gaps

Gaps where a concept-hub op EXISTS but a specific language does not have a discharged morphism
and the gap is not a structural barrier (i.e., the language has the idiom, the discharge is
failing for fixable reasons). Source: `transport-gaps.md` gap rows classified `not-supported`
(no language op spec).

| Language | Concept op | Reason |
| --- | --- | --- |
| `python` | `concept:div` | `not-supported`: python:true-division (5/2==2.5) does not transport; python:floordiv is floor-division; neither maps to concept:div (truncated-toward-zero integer) |
| `python` | `concept:bitnot` | `not-supported`: no `op_bitnot.spec.json` in python-language-signature; bitwise complement via `~x` emits `python:bitnot` but spec missing |
| `ruby` | `concept:div` | `not-supported`: operation not in supported set |
| `ruby` | `concept:shl` | `not-supported`: operation not in supported set |
| `ruby` | `concept:shr` | `not-supported`: operation not in supported set |
| `ruby` | `concept:bitnot` | `not-supported`: operation not in supported set |
| `php` | `concept:div` | `not-supported`: operation not in supported set |
| `typescript` | `concept:div` | `not-supported`: operation not in supported set |
| `go` | `concept:throw` | `not-supported`: Go uses `panic()` (unrecoverable) and `errors.New()` (error values); neither maps cleanly to concept:throw (single-value exception raise) |
| `ruby` | `concept:throw` | `not-supported` (though ruby has `raise`; discharge fails -- see transport-gaps.md) |
| `rust` | `concept:throw` | `not-supported`: Rust uses `panic!()` for unrecoverable + `Result` for recoverable; neither maps to concept:throw's effect shape |
| `go` | `concept:new` | `not-supported`: Go uses `new(T)` (zero-value allocation) and composite literals; discharge path not yet minted |
| `python` | `concept:new` | `sort-mismatch` (as of PR #742: advanced from missing-source-op to sort-mismatch; documented gap, see PR #742 architect-call flags) |
| `java` | `concept:new` | `return sort mismatch`: java:new returns `Ref`, concept:new returns `Expr`; deeper than a sort-rename, needs a `Ref extends Expr` subtype relation or a concept:new-ref variant |
| `ruby` | `concept:new` | `not-supported` |
| `go` | `concept:break` | `not-supported`: Go break takes an optional label; labeled break semantics differ from concept:break |
| `python` | `concept:break` | `effect-mismatch`: python:break emits no effects; concept:break requires `Break` + `control-transfer` |
| `python` | `concept:continue` | `effect-mismatch`: same as break |
| `python` | `concept:while` | `effect-mismatch`: python:while emits `OpaqueLoop` effect; concept:while requires `Loop` effect-polymorphic rule |

This table covers gaps where the idiom EXISTS in the language and the gap is a signature
fixup, not a missing concept. The operation-layer audit (`2026-05-12-concept-library-completeness-probe-operation-layer.md`) has full per-language sort-mismatch and precondition-mismatch rows.

---

## 4. Known Transport-Gap Mementos Already Documented

Semantic restrictions recorded in `transport-gaps.md` Semantic Restrictions section.
Each of these is an intentional non-gap -- transport correctly refuses, per design.

| Idiom / Op | Decision |
| --- | --- |
| `concept:foreach` | Demoted: no common iterator protocol across 10 languages; cross-language foreach requires `iter/has_next/next` ops not yet emitted by any lifter |
| `concept:and`, `concept:or` | Demoted: McCarthy desugarings of `concept:ite`; per-language `eq_and_to_ite_desugar` / `eq_or_to_ite_desugar` mementos handle this |
| `python:add / ts:+` polymorphism | `concept:add` is integer-only; python/ts polymorphic add does not transport; structural barrier documented |
| `python:mod` floored remainder | `concept:mod` is truncated-toward-zero; python:mod is floored; semantically distinct, correctly refused |
| `python:mul / python:neg` polymorphism | `concept:mul/neg` are integer-only; python ops dispatch on type; correctly refused |
| `concept:ushr` | Separated from `concept:shr` (logical zero-fill vs arithmetic); correctly distinct |
| `concept:div` (float) | Integer division only; python true-division and js-style polymorphic division do not transport |

The operation-layer probe's Appendix reports zero discrepancies between morphism-spec walk
and `transport-gaps.md`. This audit found no additional undocumented semantic restrictions.

---

## 5. Recommendations for 1.8.0 (P1 List)

P1 = missing concept causing immediate transport confusion in 5+ languages with no documented
disposition and no architectural blocker.

### P1-A: `concept:assert`

**Languages**: c11, java, python, rust, zig, cpp, csharp (7+ of 10 active languages)
**Problem**: `assert` appears in every language's test and defensive-programming idiom.
Without a hub op, assertion-heavy programs produce opaque unmapped rows that look like
transport failures, not intentional gaps.
**Distinction from `concept:throw`**: `assert(cond)` is a conditional abort with an implicit
condition check; it is NOT a user-initiated throw of a value. The precondition is the check
itself, not a value to propagate. A failed assert halts; `throw` propagates a catchable value.
**Suggested spec**:
```json
{
  "fn_name": "concept:assert",
  "formals": ["cond"],
  "formal_sorts": [{"kind": "ctor", "name": "Bool", "args": []}],
  "return_sort": {"kind": "ctor", "name": "Stmt", "args": []},
  "post": {
    "operator": "assert", "arity": ["Bool"], "result": "Stmt",
    "wp": "if cond then skip else abort"
  },
  "effects": {"effects": [{"kind": "effect-signature", "name": "Panic"}]}
}
```
**Blocker if absent**: programs using `assert` in test suites produce `no-concept-target`
unmapped rows indistinguishable from genuinely unsupported ops.

### P1-B: `concept:switch`

**Languages**: c11, java, go, csharp, typescript, php, zig, rust (`match`) (8 of 10)
**Problem**: multi-way branch is the second most common control-flow primitive after `if`.
Without a hub op, switch/match constructs produce `no-concept-target` rows for 8 languages.
**Note**: c11:switch is `unmapped [no-concept-target]` in the operation-layer audit; java:switch
is not even listed because the java lifter lookup-miss (see R3 in operation-layer audit) masks
it. The gap is real and wide.
**Distinction from `concept:conditional`**: conditional is binary (then/else). Switch is
multi-way over a scrutinee. They are not the same shape.
**Architectural note**: `concept:switch` with `arms: ListOfArm` introduces a `ListOfArm` sort.
The spec should define `Arm = (pattern: Expr, body: Stmt)` as a named arity shape, consistent
with the `set` arity shape used in `concept:call` and `concept:new`.

### P1-C: `concept:try` (try/catch)

**Languages**: java, typescript, php, python, ruby, csharp (6 of 10)
**Problem**: `concept:throw` names the raise side but there is no hub op for the handle side.
Programs with try/catch produce `no-concept-target` for the try-block construct, making the
exception-flow transport incomplete. `concept:throw` existing without `concept:try` is like
having `concept:conditional` without `concept:return` -- the flow is half-named.
**Note**: Rust does not use try/catch (uses `?` and `Result`); Go does not use try/catch (uses
error return values). For those languages the absence is correct. For the 6 languages that do,
it is a gap.
**Suggested spec**:
```json
{
  "fn_name": "concept:try",
  "formals": ["body", "handler"],
  "formal_sorts": [
    {"kind": "ctor", "name": "Stmt", "args": []},
    {"kind": "ctor", "name": "Stmt", "args": []}
  ],
  "return_sort": {"kind": "ctor", "name": "Stmt", "args": []},
  "post": {
    "operator": "try", "arity": ["Stmt", "Stmt"], "result": "Stmt",
    "wp": "wp(body, post) with Panic effect caught and handled by handler"
  },
  "effects": {
    "effects": [
      {"kind": "effect-polymorphic", "rule": "body.effects minus caught"},
      {"kind": "effect-polymorphic", "rule": "handler.effects"}
    ]
  }
}
```
**Architectural note**: the `minus caught` rule requires an effect subtraction operation not
currently in the effect algebra. This may push `concept:try` to P2 if effect subtraction needs
a design call. Flag for 1.8.0 planning; if effect subtraction is deferred, `concept:try` moves
to P3.

### P1-D: `concept:null-literal`

**Languages**: c11, java, python (`None`), typescript (`null/undefined`), php, ruby (`nil`),
go (`nil`), csharp (9 of 10; absent in rust which uses `Option<T>`)
**Problem**: null/nil/None is one of the most frequent terminal expression nodes in real
programs. Without a hub op, null-producing expressions produce `no-concept-target` rows for
9 languages. The operation-layer audit lists `c11:null` as `unmapped [no-concept-target]`.
**Note**: Rust's absence of null is architecturally significant and should be called out
explicitly in the spec as a language-set exclusion.
**Suggested spec**:
```json
{
  "fn_name": "concept:null",
  "formals": ["unit"],
  "formal_sorts": [{"kind": "ctor", "name": "Unit", "args": []}],
  "return_sort": {"kind": "ctor", "name": "Expr", "args": []},
  "post": {
    "operator": "null", "arity": ["Unit"], "result": "Expr",
    "wp": "null reference value"
  },
  "effects": {"effects": []}
}
```

---

## 6. Research Items for 1.9.0+

P3 items require architectural decisions before minting. Each blocks on a structural primitive
not yet in the algebra.

### P3-A: Iterator protocol (`concept:iter`, `concept:has-next`, `concept:next`)

**Prerequisite**: defines a 3-op iterator triple. Required before `concept:foreach` can be
undemotion-promoted. `concept:foreach` was demoted precisely because this triple was absent.
**Architectural call needed**: should iterator state be explicit in the sort algebra (an `Iter<T>`
sort), or should the triple be contract-only with state implicit?

### P3-B: Async/await (`concept:async-fn`, `concept:await`)

**Present in**: typescript, python, rust, csharp, java (virtual threads / CompletableFuture)
**Prerequisite**: a `Future<T>` sort in the algebra; effect-signature for async-context
suspension. `concept:await` and `concept:async-fn` are a dual pair (construction + suspension)
analogous to `concept:throw` and `concept:try`.
**Architectural call needed**: does the substrate track async execution boundaries? If yes, the
effect system needs an `AsyncSuspend` effect. If no, async is treated as opaque calls.

### P3-C: Concurrency primitives (`concept:spawn`, `concept:channel-send`, `concept:channel-recv`)

**Present in**: go (goroutines + channels), rust (threads + mpsc), typescript (promises + workers),
java (threads + queues), python (threading + asyncio)
**Prerequisite**: inter-thread causality tracking; a channel sort; a thread-handle sort.
**Architectural call needed**: concurrency in the substrate requires a decision on whether the
proof system tracks inter-thread happens-before relations. Without that decision, minting
`concept:spawn` produces correct syntactic coverage but no semantic guarantee.

### P3-D: Higher-order collection ops (`concept:map-op`, `concept:filter-op`, `concept:fold-op`)

**Present in**: all 10 languages
**Prerequisite**: a `FnType<A,B>` sort (function-as-argument); not currently in the sort algebra.
The `concept:call` spec uses `FnContract` but that is a resolved callee contract, not a
first-class function value sort. Minting higher-order ops requires extending the sort algebra
to support function types as formals.

### P3-E: Ownership/borrow (`concept:move`, `concept:borrow`)

**Present in**: rust (ownership + borrow checker), cpp (move semantics), zig (comptime ownership)
**Prerequisite**: a lifetime sort and an ownership-state sort. These do not exist in the current
algebra. Without them, `concept:borrow` is syntactically nameable but semantically empty.

### P3-F: Manual-memory primitives (`concept:alloc`, `concept:free`)

**Present in**: c11, cpp, zig (explicit allocators), rust (custom allocators)
**Relationship to `concept:new`**: `concept:new` has `effect: Alloc` and represents typed
construction. `concept:alloc` would represent raw heap reservation without type information.
These are distinct: `concept:new` = typed Alloc; `concept:alloc` = untyped Alloc.
**Architectural call needed**: does the hub distinguish typed and untyped allocation? The
current `Alloc` effect-signature exists but there is no concept op named `alloc`.

### P3-G: Literal nodes (`concept:int-literal`, `concept:bool-literal`, `concept:string-literal`)

These are P2 for completeness but P3 for sequencing -- they are inert (no effects) and
simple to spec, but the operation-layer audit suggests the genus for literal nodes (c11 has
`char_literal`, `float_literal`, `string_literal` all unmapped) has not been addressed as a
category. A literal-node sub-spec series would reduce hundreds of `no-concept-target` rows
in the lifter output. Suggest batching these as a single mint pass in 1.9.0 after iterator
and async decisions are made (to avoid constant spec-set churn).

---

## 7. Audit Limitations

### 7.1 Corpus Sources

The "programming idioms" corpus for this audit was derived from:
1. Per-language `op_*.spec.json` files inspected directly: c11 (full), python (full), go (full),
   rust (partial). The operation-layer audit provided full tables for c11/java/python.
2. Transport-gaps.md gap rows (machine-generated from 10 languages).
3. General knowledge of language idioms for csharp, typescript, php, ruby, java, zig -- NOT
   from direct inspection of each language-signature specs directory. The audit declares
   these as knowledge-based (not spec-walked) in the per-row "Languages" column.

### 7.2 What Was NOT Swept

- **cpp, swift**: directories exist but are not actively minted. Idioms for these languages
  were noted anecdotally but not counted toward the "3+ languages" threshold unless they
  appeared in the 10 active languages already.
- **aarch64, evm-bytecode, jvm-bytecode**: assembly/bytecode tiers. Not relevant to
  source-language idiom coverage.
- **Type-system operations**: generics/templates, type aliases, interface/trait declaration
  nodes. These are not operational nodes in the current hub design (the hub is control-flow +
  data-manipulation, not type-declaration). Deliberately excluded.
- **Module/import/export**: source-unit organization ops (`import`, `use`, `include`). The
  hub's `concept:source-unit` covers source-bytes wrapping, not module resolution. Module ops
  were not swept.
- **Exception hierarchy / catch-type matching**: `concept:try` as suggested in P1-C handles
  the single-handler case. Typed catch (`catch (IOException e)`) requires pattern-matching on
  exception type, which is not covered by the P1-C suggestion. This is a research item.
- **Object-oriented structural nodes**: class declaration, interface declaration, method
  declaration. These are not operational nodes; they are structural scope markers. Not swept.

### 7.3 Relation to Same-Day Operation-Layer Audit

The operation-layer audit covers discharge gaps for existing concept-hub ops. This audit
covers concept-hub ops that do not exist yet. Together they give a two-sided picture:
the op-layer audit asks "can we discharge the ops we have?" and this audit asks "what ops are
we missing?" The two audits share a baseline commit and cids.tsv hash; they should be read
as a pair.

---

> T Savo | task #69
