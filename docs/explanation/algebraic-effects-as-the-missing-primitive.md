# Algebraic Effects as ProofIR's Missing Primitive

**Status:** load-bearing architectural design.
**Author:** T Savo
**Date:** 2026-05-09

## The convergence

If you start enumerating what ProofIR can't yet express for kernel-class verification, the list looks like this:

- Heap predicates (separation logic)
- Lock state, RCU read sections, atomic memory operations
- Concurrency / interleaving / memory ordering
- Effect tracking beyond a side-metadata bag
- Generators (Python `yield`)
- Channels (Go `ch <- x` / `<- ch`)
- Mutexes (Rust `m.lock()` / `drop(g)`)
- Semaphores (P/V)
- async/await (suspend on completion)
- Throw/catch (suspend by aborting, "resume" the catch handler)
- malloc/free (resource lifecycle)
- Iterator protocols
- Continuations
- Coroutines

It looks like a lot. **It's one thing.**

Every entry on this list is an instance of:

> **A computation that suspends at a known operation, hands control to a co-operating context, and resumes (or doesn't) under a defined protocol.**

A generator suspends at `yield`, hands control to the caller, resumes on `next()`. A channel suspends at `send`, hands control to a receiver, resumes when the receive happens. A mutex suspends at `lock`, hands control to whoever holds the resource, resumes on release. A throw suspends by aborting the rest of the computation, hands control to the catch handler, never resumes the throw site. malloc suspends until the allocator can return a region. async/await suspends until the awaited future completes.

This pattern has a name in modern PL theory: **algebraic effects + handlers**. Plotkin & Power formalized it in the early 2000s. Bauer & Pretnar gave it operational semantics. It powers Koka, OCaml 5's effect system, the Eff and Frank languages, and is the unifying frame for all of the above features.

## What ProofIR is missing

ProofIR currently has:
- `IrFormula::{Atomic, And, Or, Not, ...}` over `IrTerm::{Var, Const, Ctor, Lambda, ...}`
- `Sort` for primitive types
- Quantifiers (`forall`)
- Effects as side metadata on contracts (`EffectSet`), not as first-class formula constructors

What it needs to add:

```rust
enum IrFormula {
    // ... existing constructors ...

    /// An effect operation invocation. The signature CID identifies
    /// the canonical (across-language, across-binding) effect this
    /// operation belongs to. Args are the operation's payload.
    EffectOp {
        signature_cid: String,
        operation: String,       // e.g. "yield", "send", "acquire"
        args: Vec<IrTerm>,
    },

    /// A handler for effect operations. Body is the protected
    /// computation; clauses describe how each named operation under
    /// the signature is reinterpreted, with continuation `k` available.
    Handler {
        signature_cid: String,
        body: Box<IrFormula>,
        clauses: Vec<HandlerClause>,
    },

    /// Universal quantification over a continuation value, for
    /// expressing handler clauses' resume semantics.
    ForallContinuation {
        binder: String,
        result_sort: Sort,
        body: Box<IrFormula>,
    },
}

enum IrTerm {
    // ... existing ...

    /// A continuation value. Concrete handler clauses bind these
    /// and either invoke (resume), abandon (no resume), or invoke
    /// multiple times (delimited control).
    Continuation {
        result_sort: Sort,
        signature_cid: String,
    },
}
```

And a new content-addressed memento class:

```
EffectSignatureMemento {
    signature_cid: BLAKE3-512,
    name: String,                     // "Yield", "Send", "Acquire", ...
    operations: [
        { name: "...", input_sort: Sort, resume_sort: Sort, exception_sort: Option<Sort> }
    ],
    algebraic_laws: [
        { kind: "commutativity"|"associativity"|"idempotence"|...,
          body: IrFormula }            // expressed in the same effect signature
    ],
    refines: Option<BLAKE3-512>        // signature this one refines
}
```

## What this collapses

The earlier "tower of constructors" framing — separation logic, lock state, RCU semantics, hyperproperty constructors, memory model algebra, channel semantics, generator semantics — collapses into:

> **Register the effect signature in the catalog. Every language port that wants to claim it implements that effect publishes a binding. The contracts compose mechanically via the signature CID.**

Not new ProofIR constructors. New mementos in a content-addressed catalog.

| Concrete primitive | Effect signature CID (canonical) |
|---|---|
| Python `yield` | `Yield(value: T) → resume: void` |
| Go `ch <- v` | `Send(channel: Ch, value: T) → resume: void` |
| Go `v := <- ch` | `Receive(channel: Ch) → resume: T` |
| Rust `m.lock()` | `Acquire(lock: L) → resume: Guard<L>` |
| Rust `drop(g)` | `Release(guard: Guard<L>) → resume: void` |
| C `malloc(n)` | `Alloc(size: usize) → resume: ptr` |
| C `free(p)` | `Free(ptr) → resume: void` |
| C `pthread_mutex_lock(m)` | `Acquire(lock: L) → resume: void` (same CID as Rust mutex!) |
| C `rcu_read_lock()` | `EnterReadSection(domain: rcu) → resume: void` |
| Java `throw X` | `Raise(exn: X) → resume: never` |
| OCaml/Koka `perform Ask` | `Ask → resume: T` |
| async/await `await f` | `Await(future: F<T>) → resume: T` |

**Two languages binding to the same CID compose naturally.** Cross-FFI reasoning becomes mechanical: a Python generator passing through a Rust binding to a C iterator all reference the same `Yield` CID; their contracts join via that CID.

## What this means for the kernel sweep

Most of what we want to express about kernel C is effects:
- Lock state at a callsite: handler clauses for `Acquire`/`Release` over the relevant lock signature
- RCU: handler clauses for `EnterReadSection`/`ExitReadSection`
- Memory ordering: laws on the atomic effect signatures
- Aliasing: handlers for `Read`/`Write` operations whose signatures track the heap region
- Concurrency races: handler clauses observing parallel composition of effect operations

Once ProofIR supports algebraic effects + handlers + the catalog, the substrate's expressive power for kernel verification leaps from "predicates over pure values" to "predicates over computational protocols including locks, RCU, memory ordering, and concurrency" — all via one structural primitive.

## What this means for cross-abstraction federation

A single effect signature CID can be bound by:
- Source-level constructs (the language's syntactic primitive)
- IR-level constructs (the compiler's lowering)
- Bytecode-level constructs (the runtime's representation)
- Assembly-level constructs (the ISA's atomic instruction)

Each layer publishes its binding. The substrate composes claims across layers via the signature CID. **Compilation correctness becomes a question of "do source and target bind to the same effect signatures at corresponding callsites" — not a separate proof obligation per compiler pass.**

## Why this is the deepest answer

Earlier in the design conversation we considered:
- Adding more constructors (separation logic, temporal logic, ...)
- Adding `Foreign` embedding for arbitrary other formal systems
- Adding internal universes for reflection

All of those have value. **None of them is the load-bearing missing piece.** The load-bearing piece is algebraic effects + handlers + content-addressed effect signature catalog, because:

1. It collapses an apparently-unbounded list of "things to add" into a single primitive plus a catalog
2. The catalog is the substrate's vocabulary for computational interaction; growing the catalog is composable
3. Federation across languages and abstraction layers becomes mechanical, not bespoke
4. It's well-studied (Plotkin–Power, Bauer–Pretnar, Koka, OCaml 5, Eff, Frank); we don't have to invent the theory

Reflection / Foreign embedding (the earlier "deepest answer") is then a particular use of the catalog: register a `Foreign` effect signature whose handlers translate to/from the embedded formal system. Not a separate primitive.

## Implementation roadmap

This is multi-PR architectural work. Sketched, not committed:

- **PR 1** — extend `provekit-ir-types` with `EffectOp`, `Handler`, `Continuation`. Update IR-codegen.
- **PR 2** — define the `EffectSignatureMemento` shape; mint canonical signatures (Yield, Send, Receive, Acquire, Release, Alloc, Free, Raise, Await) into the catalog at v1.7.0
- **PR 3** — extend `provekit-ir-compiler-smt-lib` to encode effect operations as uninterpreted functions with refinement axioms from the signature's algebraic laws
- **PR 4** — extend `provekit-ir-compiler-coq` to encode via interaction trees / free monads
- **PR 5** — extend `libprovekit::compose` to compose contracts that mention effect operations under the same signature CID
- **PR 6** — language ports: extend each lifter (walk-c, walk-rust, walk-py, java production walker, etc.) to recognize its language's effect-bearing primitives and emit `EffectOp` formulas with the canonical signature CID
- **PR 7** — update the protocol catalog to v1.7.0 with the new spec entries (algebraic-effects-protocol, effect-signature-memento-grammar, effect-handler-composition-rules)

## Out of scope (this design)

- Implementing any of the PRs above
- Specific algebraic-laws schemas for each registered signature
- The exact wire shape of `EffectSignatureMemento` (will be in a protocol spec)
- Coq encoding choice (interaction trees vs free monads vs Dijkstra monads)

T Savo
