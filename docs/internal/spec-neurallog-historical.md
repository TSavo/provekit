<!-- This spec was written under the product's old name (neurallog); the implemented system is ProveKit. -->

# neurallog

**A logger that fixes your code.**

neurallog.app

## Thesis

Every log statement is an implicit assertion — an informal claim about what the programmer expected to be true at that moment in execution. Traditionally, these claims are verified by human eyeballs after the fact. neurallog automates the eyeballs.

Logging is assertions made by eyeballs after the fact.

## What It Looks Like

There is no neurallog API. There is no new logging framework. The programmer writes what they've always written:

```python
logger.info(f"User balance after withdrawal: {balance}")
```

```javascript
console.log("Processing order", orderId, "for", customer.name)
```

```go
slog.Info("reservation complete", "product", productId, "qty", quantity)
```

```python
logger.debug("Reached payment step")
```

```java
log.info("Transaction complete: {} items for ${}", count, total)
```

These are ordinary log statements. Observational. "Here's a value." "We reached this point." "This happened." The programmer is not writing assertions. They're not describing invariants. They're just logging, the way they always have.

neurallog hooks the existing logging framework and treats every log statement as an intent signal — the programmer pointing at a moment and saying "this matters." The system reads the surrounding code, derives what should be true at that moment, and formally proves it.

The programmer doesn't know neurallog is there. Their code doesn't change. Their log statements work exactly as before. Behind the scenes, every one of them becomes a formally verified invariant.

This is the "do as I mean, not as I say" logging framework.

## Core Concept

The log statement is not the assertion. It is not the data format. It is not even the context. It is the **intent signal** — a marker left by a programmer at a moment they cared about.

Everything else, the system figures out:

- **What to check** — the LLM reads the surrounding code and derives what should be true at this point in execution
- **What to capture** — the contract's invariant references variables in scope; those variables are grabbed from the stack frame
- **How to verify** — Z3 formally proves the invariant holds or produces a certificate of violation
- **What to do about failure** — contradictions loop back to the LLM for resolution

The programmer writes a log line. The system produces a formal proof.

## Three Primitives

The entire system is one pattern repeated at every level:

### Context

A snapshot of state at a moment in time. Two forms, same primitive:

- **Static context** — source code, types, call chains, data flow. Captured by the discovery agent during contract derivation.
- **Runtime context** — live values from the stack frame. Captured by the hook when a log call fires.

### Contract

A formal description of a function's behavior, structured to serve the axiom templates. The schema is derived from what the axiom templates consume:

```json
{
  "function": "reserve_stock",
  "file": "src/inventory.py",
  "line": 11,

  "preconditions": [
    {
      "claim": "quantity must be positive",
      "smt2": "(> quantity 0)",
      "variables": [{"name": "quantity", "type": "Int", "source": "parameter"}]
    },
    {
      "claim": "quantity must not exceed available stock",
      "smt2": "(<= quantity available)",
      "variables": [
        {"name": "quantity", "type": "Int", "source": "parameter"},
        {"name": "available", "type": "Int", "source": "db.get_available(product_id)"}
      ]
    },
    {
      "claim": "available stock is non-negative",
      "smt2": "(>= available 0)",
      "variables": [{"name": "available", "type": "Int", "source": "db.get_available(product_id)"}]
    }
  ],

  "postconditions": [
    {
      "claim": "available decreases by quantity",
      "smt2": "(= new_available (- available quantity))",
      "variables": [
        {"name": "new_available", "type": "Int"},
        {"name": "available", "type": "Int"},
        {"name": "quantity", "type": "Int"}
      ]
    },
    {
      "claim": "reserved increases by quantity",
      "smt2": "(= new_reserved (+ reserved quantity))",
      "variables": [
        {"name": "new_reserved", "type": "Int"},
        {"name": "reserved", "type": "Int"},
        {"name": "quantity", "type": "Int"}
      ]
    },
    {
      "claim": "stock total is conserved",
      "smt2": "(= (+ new_available new_reserved) (+ available reserved))",
      "variables": [
        {"name": "new_available", "type": "Int"},
        {"name": "new_reserved", "type": "Int"},
        {"name": "available", "type": "Int"},
        {"name": "reserved", "type": "Int"}
      ]
    }
  ],

  "side_effects": [
    {
      "target": "db.available",
      "key_field": "product_id",
      "operation": "write",
      "transition": "(= new_available (- available quantity))"
    },
    {
      "target": "db.reserved",
      "key_field": "product_id",
      "operation": "write",
      "transition": "(= new_reserved (+ reserved quantity))"
    }
  ],

  "domain_constraints": [
    {
      "claim": "available stock should remain non-negative",
      "smt2": "(>= new_available 0)"
    },
    {
      "claim": "reserved stock should remain non-negative",
      "smt2": "(>= new_reserved 0)"
    }
  ],

  "visibility": "public",
  "idempotency_guard": null,

  "clause_history": [
    {
      "clause": "(> quantity 0)",
      "status": "active",
      "weaken_step": 0,
      "witness_count_at_last_weaken": 0,
      "current_witness_count": 847
    },
    {
      "clause": "(<= quantity available)",
      "status": "weakened",
      "weaken_step": 3,
      "witness_count_at_last_weaken": 12,
      "current_witness_count": 42
    }
  ],

  "provenance": {
    "derived_at": "2026-04-14T03:22:41Z",
    "model": "llama3:70b",
    "prompt_hash": "c4f2a...",
    "file_hash": "a3f8c2e1...",
    "principle_hash": "7b3d1..."
  }
}
```

#### Schema Fields

Each field exists because at least one axiom template requires it:

| Field | Required by | Purpose |
|---|---|---|
| `preconditions` | P1, P2, P3, P4 | What must be true before calling this function |
| `postconditions` | P1, P2, P4 | What the function guarantees after execution |
| `side_effects` | P2, P4 | What shared state the function mutates, keyed by what identity |
| `side_effects.key_field` | P2 | The data-dependent resource identity (e.g., product_id) — determines if loop iterations alias |
| `domain_constraints` | P5 | What values are semantically meaningful — the only LLM-dependent field |
| `visibility` | P3 | Whether the function is public (determines if inputs are unconstrained) |
| `idempotency_guard` | P4 | Whether the function checks state before acting (e.g., `if status == "done": return`) — null means no guard |
| `clause_history` | Termination engine | Per-clause weaken/strengthen history with witness-count stamps. Required for the well-founded ordering that guarantees convergence termination. Persisted across runs in `.neurallog/contracts/`. Without this, the termination argument degrades to "we hope the LLM respects a rule it can't see." |

Every `smt2` field is a valid SMT-LIB 2 expression. Every `variable` has a name, type, and source (parameter, local computation, database read, return value of another function). The `claim` fields are natural language for humans — never used in mechanical reasoning.

#### Why This Shape

The contract is not a single predicate. It is a structured object with distinct fields for preconditions, postconditions, side effects, domain constraints, visibility, and idempotency. This structure exists because different axiom templates need different fields:

- P1 reads `callee.preconditions` and checks them against `caller.postconditions`
- P2 reads `F.side_effects.key_field` to detect loop aliasing, then uses `F.preconditions` and `F.postconditions` to model two iterations
- P3 reads `visibility` to determine if inputs are unconstrained, then checks all reachable `preconditions`
- P4 reads `idempotency_guard` to determine if double-invocation is possible, then uses `postconditions` to model the state after the first call and checks `preconditions` against it
- P5 reads `domain_constraints` — the semantic bounds that require LLM derivation
- P6 checks `postconditions` when collection sources are empty
- P7 inspects `preconditions` for arithmetic operand bounds

A flat predicate-and-claim contract couldn't serve these templates. The structure is the interface between Layer 1 (LLM derives) and Layer 2 (Z3 reasons).

Contracts are versioned, auditable artifacts with full provenance: what code the LLM saw, what prompt was used, what model was used, what file hash and principle hash the derivation was based on.

### Evaluation

A contract applied to a context, producing evidence:

- **Pass** — Z3 proof certificate that the invariant holds
- **Fail** — Z3 proof of violation with unsat core identifying exactly which constraints conflict

Every level of the system is Context + Contract = Evaluation. This is why the system self-hosts cleanly: contract derivation is itself an evaluation (static context + meta-contract = derivation record). It's proofs all the way down.

## Runtime-First Derivation

Contracts are not derived in a separate analysis pass. They are derived **the first time a log statement fires at runtime**. The system bootstraps itself from actual execution.

### First Execution (Cold Start)

1. `logger.info(f"User balance after withdrawal: {balance}")` fires for the first time
2. The hook intercepts the call
3. It computes the call site identity: file path + line number
4. It checks the contract cache: is there a contract for this call site with a matching file hash?
5. **Cache miss** — no contract exists
6. The hook captures the file path and line number, sends them to the invariant service
7. The invariant service triggers the discovery agent, which explores the code around that call site — types, function signature, call chains, data flow
8. The LLM derives a contract: predicate + claim
9. The contract is cached, keyed by `file_path:line_number:md5(file_contents)`
10. On this first call, the hook also inspects the stack frame, evaluates the fresh contract, and records the first proof entry

### Subsequent Executions (Cache Hit)

1. The same log statement fires again
2. The hook intercepts, computes call site identity
3. Cache hit — contract exists and file hash matches
4. The hook inspects the stack frame, grabs the values the contract needs
5. Z3 evaluates the predicate
6. Proof entry recorded

This is microseconds. No LLM in the loop. No network call. Just a cache lookup, a frame inspection, and a Z3 evaluation.

### Invalidation

When the source file changes, the MD5 hash changes. The next time the log statement fires, the cache misses — the stored hash doesn't match the current file. The contract is stale. Re-derive.

This means contracts are always consistent with the current code. Staleness is detected automatically, at runtime, with zero configuration. No file watchers. No git hooks. No CI integration required. The cache just does the right thing.

### Mechanical Walkthrough

Here's exactly what happens, step by step, the first time the application runs.

**The application starts:**

```python
import neurallog
```

This installs a hook on Python's `logging` module. Every `logger.info()`, `logger.debug()`, etc. passes through neurallog before the normal log output. The programmer's logging still works exactly as before.

**First log call fires:**

```python
# inventory.py, line 15
def check_availability(product_id):
    available = db.get_stock(product_id)
    logger.info(f"Stock check for {product_id}: {available} available")
    return available
```

1. **Identify the call site.** The hook grabs the caller's frame: `frame.f_code.co_filename` → `src/inventory.py`, `frame.f_lineno` → `15`. Call site: `src/inventory.py:15`.

2. **Check the contract cache.** Look for `src/inventory.py` in `.neurallog/contracts/`. Compute MD5 of the source file. Either the contract file doesn't exist, or the hash doesn't match. Cache miss.

3. **Capture the frame immediately.** The frame is ephemeral — it won't exist once this function returns. Grab everything now: `dict(frame.f_locals)` → `{ "product_id": "SKU-123", "available": 47 }`.

4. **Let the original log call through.** The programmer sees their log line in stdout. neurallog is invisible. No blocking.

5. **Kick off derivation in the background.** The invariant service receives the file path and line number.

6. **Discovery agent runs.** It reads `src/inventory.py`, finds line 15 inside `check_availability`. It examines the function signature, the `db.get_stock` call, the return value, the callers in other files. It assembles a context bundle.

7. **LLM derives the contract.** The context bundle goes to ollama. The LLM produces:
   - Predicate: `(>= available 0)`
   - Claim: "available stock count is non-negative"
   - Variables needed from frame: `["available"]`
   - Confidence: 0.95

8. **Write to disk.** The contract is written to `.neurallog/contracts/src/inventory.py.json`:

```json
{
  "file_hash": "a3f8c2e1...",
  "contracts": [
    {
      "line": 15,
      "function": "check_availability",
      "predicate": "(>= available 0)",
      "claim": "available stock count is non-negative",
      "variables": ["available"],
      "confidence": 0.95,
      "provenance": {
        "model": "llama3:70b",
        "context_hash": "b7d1f...",
        "discovered": [
          "db.get_stock returns integer stock count",
          "returned to pricing.py:calculate_price"
        ],
        "derived_at": "2026-04-14T03:22:41Z"
      }
    }
  ]
}
```

9. **Evaluate retroactively.** We captured the frame in step 3. We now have the contract. Contract says check `available` against `(>= available 0)`. Frame has `available = 47`. Z3 confirms: invariant holds. Proof certificate generated. First proof entry recorded.

**Second log call fires (different call site, same file):**

```python
# inventory.py, line 25
def reserve_stock(product_id, quantity):
    available = db.get_available(product_id)
    reserved = db.get_reserved(product_id)
    logger.info(f"Reserving {quantity} of {product_id} ({available} available, {reserved} reserved)")
```

1. Hook intercepts. Call site: `src/inventory.py:25`. The contract file for `inventory.py` exists but has no contract for line 25. Cache miss for this call site.

2. Capture frame: `{ "product_id": "SKU-123", "quantity": 5, "available": 47, "reserved": 12 }`. Let the log through.

3. Discovery agent runs. It reads the code around line 25. **But now something is different: there's already a contract for line 15 in this file.** The agent sees it in `.neurallog/contracts/src/inventory.py.json`. The existing contract says `available >= 0`. The agent incorporates this as known context. The LLM has a richer picture — it knows what's already been proven.

4. LLM derives the contract for line 25, informed by the existing contract:
   - Predicate: `(and (> quantity 0) (<= quantity available) (>= available 0) (>= reserved 0))`
   - Claim: "reservation quantity is positive and does not exceed available stock"

5. The new contract is appended to `.neurallog/contracts/src/inventory.py.json`.

6. **Cross-contract consistency check.** Z3 gets both contracts from this file and checks: can they both be true? `sat` → consistent. Consistency proof written to `.neurallog/consistency/`.

7. Evaluate retroactively against captured frame. Pass. Proof entry recorded.

**First call site fires again:**

```python
# inventory.py, line 15 — second execution
logger.info(f"Stock check for {product_id}: {available} available")
```

1. Hook intercepts. Call site: `src/inventory.py:15`.
2. Check cache. Contract file exists. File hash matches. Contract for line 15 exists. **Cache hit.**
3. Read the contract. It needs `available` from the frame.
4. Inspect frame. `available = 31`.
5. Z3 evaluates. `31 >= 0`. Pass. Microseconds.
6. Proof entry recorded. No LLM. No network. No discovery. Just a cache lookup, a frame read, and a Z3 check.

**Disk state after these three calls:**

```
.neurallog/
  contracts/
    src/
      inventory.py.json       # both contracts, one file
  consistency/
    src/
      inventory.py.proof      # cross-contract consistency proof
  cache.json                  # file hashes for staleness detection
```

Each new call site adds to the contract file for its source file. Cross-contract checks get richer as more contracts exist along each code path. The system builds up a formal model of the entire codebase, one log statement at a time, driven by actual execution.

## Context Capture

The log statement does not define what gets captured. The **contract** does.

A programmer writes `logger.info(f"Unit price: {unit_price}")`. They logged one value. But the LLM reads the surrounding code and determines the real invariant is:

```
unitPrice > 0 AND unitPrice <= product.maxPrice AND currency != null
```

The contract references `unit_price`, `product.max_price`, and `currency`. At runtime, the hook inspects the stack frame and captures exactly those values — even though the programmer only logged `unit_price`.

**The log statement is the tripwire. The stack frame is the context. The contract says what to grab.**

### Stack Frame Inspection by Language

The runtime already has every value in scope at the call site. The hook just reaches in and takes what the contract needs. No source modification. No instrumentation. No build step.

- **Python** — `inspect.currentframe().f_back.f_locals` — trivial, full access to all locals
- **Java** — JVMTI / Java agents — can inspect local variables at runtime
- **C#** — Debugger APIs
- **Node.js** — V8 inspector protocol
- **Go** — runtime introspection via debug APIs

## Two Modes

### Derivation Time — Static Analysis from Log Statements

A logger that is secretly a static analyzer.

Contracts derived from all log statements along a code path are fed to Z3 as a set. If Z3 returns **unsat**, the contracts are mutually contradictory — the programmer's own implicit assumptions about their code can't all be true simultaneously.

This is a bug, found before a single line executes. Not from type rules or lint patterns — from what the programmer **believed** about their code, checked against itself. The programmer's log statements prove their code is inconsistent.

The unsat core identifies exactly which assumptions conflict.

### Runtime — Formal Proof of Running Code

At runtime, every log call produces a formally verified proof entry. The proof log is a continuous, machine-verified record that the software is doing what it claims to be doing.

When an invariant is violated, the log entry contains:
- The claim derived by the LLM (what should be true at this point)
- The formal proof of violation (Z3 certificate)
- The exact values that violated the invariant (from the stack frame)
- The unsat core (exactly which constraints conflicted)

The system degrades gracefully into being the best logger ever written. Pass = proof of correctness. Fail = the richest diagnostic that's ever existed.

## The Unsat Loop

When Z3 finds a contradiction — either at derivation time (contracts inconsistent with each other) or at runtime (values violate a contract) — the system acts on it.

One of two things **must** be wrong:

1. **Bad contracts** — the LLM over-constrained or misunderstood the code. Re-derive with the unsat core as feedback. The contracts improve.
2. **Bad code** — the invariants correctly capture the programmer's intent, but the code can't satisfy them. The LLM produces a patch.

The loop: log statements → invariants → Z3 finds contradiction → LLM resolves (fix contracts or fix code) → re-derive → Z3 confirms. The codebase converges toward provably correct.

**A logger that fixes your code.**

## Architecture

The engine is written in TypeScript. It is completely language-neutral — it parses, derives, proves, and stores without knowing or caring what language the target code is in. Language-specific behavior lives in adapters.

### The Engine (Language-Neutral)

These components have no knowledge of the target language:

**Invariant Service.** Minimal. POST code context, receive contract, cache it. Backed by an LLM (ollama). The core logic is ~8 lines. An HTTP wrapper around a prompt and a cache.

**Contract Cache.** Keyed by `file_path:line_number:md5(file_contents):md5(principles)`. When the file or principles change, the hash changes, the cache misses, the contract is re-derived.

**Prompt Assembler.** Takes the context bundle from Phase 0 and assembles the derivation prompt: base methodology + selected axiom teaching examples + existing contracts + code context + target line. The prompt template lives in `prompts/invariant_derivation.md`.

**Axiom Template Engine.** Takes axiom templates + cached contracts, mechanically instantiates Z3 checks. This is Layer 2 — the hot path. No LLM, no language knowledge. Just template parameters and Z3.

**Z3 Runner.** Feeds SMT-LIB blocks to Z3, collects results (sat/unsat + models/proofs). Validates LLM output. Runs axiom-generated checks.

**Proof Store.** Append-only ledger of evaluations. Writes to `.neurallog/contracts/`, `.neurallog/consistency/`, and the proof log transport layer.

**Principle Manager.** Manages the axiom library in `.neurallog/principles/`. Handles Phase 2: classification, generalization, self-validation, commitment of new principles.

### Language Adapters

Each target language provides an adapter implementing a common interface. The adapter handles the four things that are unavoidably language-specific:

#### The Adapter Interface

```typescript
interface LanguageAdapter {
  // What language does this adapter handle?
  language: string;
  extensions: string[];  // e.g., [".ts", ".tsx", ".js", ".jsx"]

  // Phase 0: Discovery
  parseFile(source: string): AST;
  findLogStatements(ast: AST): CallSite[];
  resolveImports(ast: AST, filePath: string): ImportedFile[];
  extractFunction(ast: AST, line: number): FunctionNode;
  findCallExpressions(fn: FunctionNode): CallExpression[];
  determineVisibility(fn: FunctionNode): "public" | "private" | "internal";

  // Runtime: Hook installation
  installHook(config: HookConfig): void;

  // Runtime: Stack frame inspection
  captureFrame(callSite: CallSite): Record<string, any>;
}
```

All six methods use tree-sitter under the hood. The tree-sitter grammar is the foundation of every adapter.

#### Tree-Sitter Foundation

Tree-sitter provides:
- **AST parsing** — source code → syntax tree, for any supported language
- **Node types** — function definitions, call expressions, import statements, loop constructs, arithmetic operators — consistent structure across languages
- **Queries** — pattern matching on the AST to find log statements, call sites, function boundaries
- **Incremental parsing** — only re-parse what changed, fast enough for runtime use

Each adapter defines tree-sitter queries for its language:

```typescript
// TypeScript adapter — log statement detection
const LOG_PATTERNS = [
  'console.log', 'console.info', 'console.warn', 'console.error', 'console.debug',
  // Popular frameworks
  'logger.info', 'logger.debug', 'logger.warn', 'logger.error',
  // Pino, Winston, Bunyan patterns
  'log.info', 'log.debug', 'log.warn', 'log.error',
];

const LOG_QUERY = `
  (call_expression
    function: (member_expression
      object: (identifier) @object
      property: (property_identifier) @method)
    arguments: (arguments) @args)
`;
```

```typescript
// TypeScript adapter — import resolution
const IMPORT_QUERY = `
  [
    (import_statement source: (string) @path)
    (call_expression
      function: (identifier) @fn (#eq? @fn "require")
      arguments: (arguments (string) @path))
  ]
`;
```

#### Import Resolution

Import resolution maps import statements to source files. This is the most language-specific part:

**TypeScript/JavaScript:**
- `import { foo } from './bar'` → resolve `./bar.ts`, `./bar.js`, `./bar/index.ts`
- `require('./bar')` → same resolution
- `import { foo } from 'some-package'` → resolve from `node_modules/` — depth-1 only includes direct dependencies, not the package's internals
- Uses Node's module resolution algorithm or TypeScript's `paths` configuration

**Python (second adapter):**
- `import inventory` → find `inventory.py` on `sys.path`
- `from inventory import reserve_stock` → same file, specific function
- Relative imports: `from . import inventory` → resolve relative to package

**Go (future adapter):**
- `import "myapp/inventory"` → resolve from `$GOPATH` or module path

Each adapter implements `resolveImports()` using its language's resolution algorithm. The engine doesn't know or care how resolution works — it just gets back a list of `ImportedFile` objects with paths and source content.

#### Stack Frame Inspection

The runtime component that captures live values when a log statement fires:

**TypeScript/JavaScript:**
- V8 Inspector Protocol — connect to the running process, set a breakpoint-like hook at the log call site, read local variables from the call frame
- Alternatively: source transformation at build time that captures `arguments` and local scope into a structured object before the log call
- Pragmatic first approach: proxy `console.log` etc., capture the arguments passed to the log call (not full scope), derive contracts initially from just the logged values, expand to full scope inspection later

**Python (second adapter):**
- `inspect.currentframe().f_back.f_locals` — one line, full access to everything in scope
- Trivial compared to every other language

The adapter interface abstracts this: `captureFrame(callSite)` returns a `Record<string, any>` regardless of how it was obtained.

### First Adapter: TypeScript

The TypeScript adapter is the first implementation. It provides:

- tree-sitter-typescript for AST parsing
- Log detection for `console.*` and popular logging libraries (pino, winston, bunyan)
- Node module resolution for import following
- V8 inspector or source transformation for frame capture
- `export` keyword for visibility detection

The engine is also written in TypeScript, so the first adapter dogfoods the language. neurallog analyzing neurallog's own TypeScript source is the self-hosting proof.

### Second Adapter: Python

The Python adapter is the second implementation — lowest effort because:
- `inspect.currentframe().f_back.f_locals` for stack frame inspection (trivial)
- Monkey-patching `logging.Handler` for the hook (trivial)
- tree-sitter-python for AST (mature grammar)
- Python's module resolution for imports (well-defined)

### Adding a Language

Writing an adapter for a new language requires:
1. A tree-sitter grammar (already exists for 100+ languages)
2. Log statement patterns for the language's logging idioms
3. Import resolution logic for the language's module system
4. Stack frame inspection using the language's runtime APIs
5. Visibility rules for the language's access model

The engine, the axiom templates, the contract schema, the proof format, Z3, the principle library — none of these change. A new language is just a new adapter.

## The Derivation Pipeline

When a log statement fires and requires contract derivation (cache miss), the system runs a three-phase pipeline:

### Phase 0: Context Assembly (Deterministic, No LLM)

Before the LLM sees anything, the system mechanically assembles the context bundle:

**Step 0a: Identify the call site.**
From the stack frame: file path, line number, function name.

**Step 0b: Parse the target file.**
Tree-sitter (or language-native AST) parses the source file. Extract:
- The target function's full source
- All function calls within the target function
- All log statements (for cross-referencing existing contracts)
- Import statements

**Step 0c: Resolve depth-1 imports.**
Follow import statements to their source files. Parse each imported file. Extract:
- Function signatures and implementations called by the target function
- Only include functions actually called — not the entire imported file

**Step 0d: Gather existing contracts.**
Read `.neurallog/contracts/` for:
- Any existing contracts in the target file (other call sites already derived)
- Contracts for imported functions (their pre/postconditions and side effects)
- Contracts for transitive dependencies (if available — contracts only, not source)

**Step 0e: Select relevant axioms.**
AST analysis of the target function determines which axioms apply:
- Has function calls? → P1 (precondition propagation)
- Has loops calling state-mutating functions? → P2 (state mutation)
- Is a public function? → P3 (calling context)
- Calls functions with side effects? → P4 (temporal analysis)
- Computes values used in business logic? → P5 (semantic correctness)
- Processes collections or accumulates values? → P6 (boundary inputs)
- Has division, subtraction, or multiplication? → P7 (arithmetic safety)
- Plus any system-generated axioms from `.neurallog/principles/`

The selection is rule-based, not LLM-based. The AST determines applicability.

**Step 0f: Assemble the prompt.**

The prompt is assembled from the template at `prompts/invariant_derivation.md` by filling its template variables:

| Template Variable | Filled By |
|---|---|
| `{{TARGET_FILE}}` | Step 0a: file path from stack frame |
| `{{TARGET_FUNCTION}}` | Step 0b: enclosing function name from AST |
| `{{TARGET_LINE}}` | Step 0a: line number from stack frame |
| `{{TARGET_STATEMENT}}` | Step 0b: the log statement source code from AST |
| `{{TARGET_FILE_SOURCE}}` | Step 0b: full source of the target file |
| `{{IMPORT_SOURCES}}` | Step 0c: source of each depth-1 imported file, filtered to functions actually called |
| `{{EXISTING_CONTRACTS}}` | Step 0d: all relevant contracts, formatted as SMT-LIB with claims |
| `{{CALLING_CONTEXT}}` | Step 0b: visibility (public/private), known callers if determinable |

The template itself contains:
- The Z3 verification methodology (prove via negation/unsat, find bugs via sat)
- The selected axiom principles with their teaching examples (from Step 0e)
- The SMT-LIB grammar constraint
- The output format specification (proven properties + reachable violations, each tagged with axiom or `[NEW]`)
- The contract schema the LLM should produce (preconditions, postconditions, side effects, domain constraints, visibility, idempotency guard)

The full prompt template is maintained at `prompts/invariant_derivation.md` and evolves as new axioms are added. Phase 0 assembles a concrete prompt from the template — deterministically, with no LLM calls.

### Phase 1: Contract Derivation (LLM, One Call)

The assembled prompt is sent to the LLM. One call per log statement.

The LLM produces:

**Proven properties** — assertions guaranteed by the code and existing contracts, expressed as self-contained SMT-LIB blocks with `(check-sat)` expecting `unsat`.

**Reachable violations** — preconditions of called functions that the target code doesn't establish, expressed as self-contained SMT-LIB blocks with `(check-sat)` expecting `sat`. Each violation is tagged with the axiom that led to its discovery, or `[NEW]` if discovered through free reasoning.

**The function contract** — the target function's preconditions, postconditions, and side effects, extracted from the analysis. This is what gets cached and used by Layer 2 (mechanical axiom application) for future derivations.

The LLM output is parsed. Each SMT-LIB block is fed to Z3 for validation:
- Proven properties: Z3 must return `unsat`. If it returns `sat`, the "proof" is wrong — discard or flag.
- Reachable violations: Z3 must return `sat`. If it returns `unsat`, the "bug" is a false positive — discard.

Only Z3-validated results are committed. The LLM proposes; Z3 verifies.

The validated contract is written to `.neurallog/contracts/`. Proof entries are recorded. Violations are routed to the appropriate handler (file bug, propose fix, alert).

### Phase 2: Principle Classification (Conditional, Rare)

Phase 2 only runs when Phase 1 produces violations tagged `[NEW]` — findings that don't match any existing axiom.

**Step 2a: Classify.**
A separate LLM call receives the `[NEW]` violation and the full axiom library. It determines: is this genuinely new, or does it actually fit an existing axiom that the Phase 1 LLM failed to tag correctly?

If it fits an existing axiom: re-tag and move on. No new principle needed.

**Step 2b: Generalize.**
If genuinely new, the LLM extracts the general pattern:
- Names the principle
- Writes a textbook-style description
- Creates a teaching example in a completely different domain
- Produces the SMT-LIB block for the teaching example

**Step 2c: Adversarial validation.**
Self-validation (same model validates its own principle) launders shared blind spots as consensus. Instead, validation is adversarial:

1. **Different model as adversary.** A different model (e.g., haiku if opus derived, or vice versa) tries to produce code that the new principle *shouldn't* flag but does (false positive), or code that *should* be flagged but isn't (false negative). The principle commits only if the adversary can't find counterexamples within 10 attempts. This is cheap (a haiku adversary against an opus derivation) and directly addresses the shared-bias problem.

2. **Historical corpus.** If the codebase has historical bug-fix commits (PRs tagged `fix:`, `bugfix:`, etc.), the new principle is tested against them. The threshold is relative, not absolute: the principle must correctly classify bugs at a rate above the base rate of the corpus. A codebase where 5% of commits are bug fixes requires the principle to flag more than 5% of bug-fix commits as violations — otherwise it's no better than random. Non-bug commits must not be flagged at a rate exceeding 1% (false positive ceiling). This is empirical grounding, not LLM self-report.

3. **Cross-file generalization.** The principle is added to the prompt and used to re-derive contracts for a different file. Does it find violations the previous principle set missed? This is the weakest check — necessary but not sufficient.

**Attempt count scales with principle stakes.** 10 adversarial attempts is not a universal constant. Principles that fire on security-critical or financial code paths deserve 100+ attempts. Cheap boundary checks are fine at 10. The attempt count is a configurable knob, defaulting to 10 and overridable per-principle based on the code paths it affects.

**False positive ceiling needs tiered enforcement.** 1% FP rate per principle sounds low, but it compounds: 30 principles × 100 PRs/week × 1% = ~30 false flags/week. That's the point where developers turn off the bot. Mitigations:

- The 1% ceiling is per-principle. Principles that exceed it are quarantined (moved to advisory-only, not blocking).
- Principles are grouped into confidence tiers: **blocking** (high confidence, security/payments), **warning** (medium confidence, general logic), **advisory** (new or low confidence, shown but not counted as violations). Developers can configure which tiers block CI.
- The system tracks per-principle FP rates from developer feedback (dismissals, overrides) and auto-quarantines principles that cross the threshold.

Adversarial validation is **empirical hardening, not soundness.** Absence of counterexamples within N attempts does not prove the principle is correct — it proves the adversary couldn't break it quickly. This is consistent with the "Why Not Refinement Types?" section: neurallog does not claim soundness. It claims useful, empirically validated, continuously improving verification. The hedges are load-bearing; do not drop them.

**Step 2d: Formalize as axiom template.**
If validated, the principle is also expressed as a formal axiom template — a parameterizable Z3 check generator that can be applied mechanically in Layer 2 without the LLM.

**Step 2e: Commit.**
The new principle is written to `.neurallog/principles/`. The principle hash changes. All contracts derived without this principle are now stale. Re-derivation happens lazily on next cache miss, or can be triggered explicitly.

### Pipeline Summary

```
Phase 0: Assemble context (deterministic, fast)
    |
    | AST parse → resolve imports → gather contracts → select axioms → build prompt
    |
    v
Phase 1: Derive contract (one LLM call)
    |
    | LLM produces proven properties + reachable violations (tagged with axioms)
    | Z3 validates each SMT-LIB block
    | Contract cached, proofs recorded, violations routed
    |
    +--> Any [NEW] tagged violations?
    |       |
    |       No → done
    |       |
    |       Yes → Phase 2
    |
    v
Phase 2: Classify and grow principles (conditional, rare)
    |
    | Classify: genuinely new or mis-tagged?
    | Generalize: extract pattern, create teaching example in different domain
    | Self-validate: test against different code
    | Formalize: create axiom template for mechanical application
    | Commit: write to .neurallog/principles/
    |
    v
Principle hash changes → stale contracts re-derive on next execution
```

### What Runs When

| Trigger | Phase 0 | Phase 1 | Phase 2 |
|---|---|---|---|
| First execution of a log statement (cache miss) | Yes | Yes | If [NEW] violations |
| Same log statement, same code, same principles (cache hit) | No | No | No |
| Code changed (file hash mismatch) | Yes | Yes | If [NEW] violations |
| New principle added (principle hash mismatch) | Yes | Yes | If [NEW] violations |
| Layer 2 mechanical axiom application (contracts exist) | No | No | No |

The common case after initial convergence: cache hit → no phases run → Z3 evaluates cached contract against runtime values → microseconds.

## The Loop

```
logger.info(f"User balance: {balance}")
    |
    | (first call: cache miss)
    v
Discovery agent explores surrounding code
    |
    v
LLM derives contract (Z3 predicate + claim)
    |
    v
Contract cached against file_path:line:md5(file)
    |
    v
Z3 checks contract set for cross-contract consistency
    |
    +--> unsat? --> LLM resolves (fix contracts or fix code)
    |
    v
    |
    | (every call: cache hit)
    v
Hook inspects stack frame, grabs values contract needs
    |
    v
Z3 evaluates predicate against live values
    |
    v
Proof entry recorded (pass with certificate, or fail with unsat core)
    |
    +--> violation? --> formal proof of failure
    |                   + loop back to LLM for resolution
    v
Proof log: continuous, formally verified record of software behavior
```

## What the Proof Log Replaces

- **Logging** — every entry is a log line. Verified ones have proofs. Failed ones have formal diagnostics.
- **Assertions** — contracts are assertions derived from code context, not manually written.
- **Monitoring/alerting** — contract violations are alerts grounded in formal proofs.
- **Static analysis** — cross-contract consistency checking finds bugs before runtime, from the programmer's own implicit beliefs about their code.
- **Compliance/auditing** — the proof log is a machine-verifiable record of runtime behavior. Not "trust our monitoring" but "here, verify it yourself."

## Two Entry Points, Same Engine

neurallog has two entry points. Both use the same engine — same contracts, same axioms, same Z3, same proof format. One runs without the application. One runs with it.

### Dev/CI Mode: Static Analysis from Log Statements

```bash
neurallog analyze src/
```

No application running. No hooks. No frame inspection. Pure static analysis.

The engine:
1. Tree-sitter parses every source file in the target directory
2. Finds every log statement across all files
3. Resolves imports, builds the call graph
4. Derives contracts for every function containing a log statement (LLM, cached)
5. Applies axiom templates mechanically to all contract pairs (Z3)
6. Checks cross-file consistency of the full contract set (Z3)
7. Reports: proven properties, reachable violations, coverage

The output is provably correct code. From log statements you already have.

This runs in CI as a check. If any reachable violations are found, the check fails. If all axiom applications return `unsat`, the code is formally verified for every property the contracts cover.

```bash
# CI integration
neurallog analyze src/ --ci          # exit code 1 if violations found
neurallog analyze src/ --report      # generate proof report
neurallog analyze src/ --diff HEAD~1 # only analyze changed files
neurallog analyze src/ --coverage    # the map
```

The `--diff` mode is the fast path for CI: only re-derive contracts for files that changed, then re-run axiom application against the full contract set. Changed code gets cached contracts. Axiom application is always mechanical and fast.

The `--coverage` mode produces the map promised in the adoption narrative — the concrete artifact that shows what's verified and what isn't:

```
neurallog coverage report: src/
──────────────────────────────────────────
Log statements found:          347
  ├─ Strong contracts:         142  (41%)  ← formally verified
  ├─ Weak contracts (DB/IO):    89  (26%)  ← local invariants only, no cross-function composition
  ├─ Opaque (higher-order):     61  (18%)  ← call site not statically resolvable
  └─ Trivial (true):            55  (16%)  ← nothing to prove

Axiom coverage:
  P1 Precondition propagation:  87 checks, 12 violations
  P2 State mutation:            23 checks,  4 violations
  P3 Calling context:           42 checks,  8 violations
  P4 Temporal:                  15 checks,  3 violations
  P5 Semantic:                  31 checks,  1 violation
  P6 Boundary:                  19 checks,  5 violations
  P7 Arithmetic:                44 checks,  0 violations
  P8 Atomicity:                  9 checks,  2 violations

Files with most unverified call sites:
  src/handlers/webhook.ts       14 opaque (callback-heavy)
  src/services/payment.ts        8 weak   (DB reads, no transaction annotations)
  src/utils/transform.ts         0 gaps   (fully verified)
```

This is the "you've never had this map before" artifact. It shows exactly where to invest: annotate transactions in payment.ts, refactor callbacks in webhook.ts, and leave transform.ts alone — it's already proven.

### Production Mode: Runtime Verification via Logging Transport

```typescript
// TypeScript — add a pino transport
import pino from 'pino';
const logger = pino({ transport: { target: 'neurallog' } });
```

```python
# Python — add a logging handler
import logging
from neurallog import NeuralLogHandler
logging.root.addHandler(NeuralLogHandler())
```

The transport hooks into the existing logger. When a log call fires:

1. The transport intercepts the call
2. Captures the stack frame (live values in scope)
3. Ships the values to the neurallog engine
4. The engine evaluates cached contracts against the live values
5. Z3 produces a proof entry (pass or fail)
6. The proof entry is recorded and/or streamed to configured transports

The original log call still executes normally. neurallog is invisible to the programmer and to the application.

Production mode does everything dev mode does, plus:
- **Runtime value verification** — contracts checked against actual values, not just static analysis
- **Proof log** — continuous, formally verified record of runtime behavior
- **Live violation detection** — bugs caught in production with formal proofs

### The Relationship Between Modes

Dev mode finds bugs before you ship. Production mode proves correctness while you run.

Dev mode produces **static proofs** — "for all possible values satisfying the preconditions, this property holds." These are universal guarantees derived from the code structure.

Production mode produces **runtime proofs** — "at this moment, with these specific values, this property held." These are concrete evidence of correctness for each execution.

Both kinds of proof live in the same proof store, use the same format, and are independently verifiable with Z3.

A codebase that passes `neurallog analyze` with zero violations is formally verified at the static level. A codebase that additionally runs the production transport and sees zero runtime violations is formally verified at both levels — the code is correct in theory AND in practice.

### Runtime Deployment Modes

Within production mode, the transport can operate in three sub-modes:

**Local Gate (Fail Fast).** Z3 evaluates contracts synchronously at the call site. When an invariant is violated, it throws. The log statement becomes a hard gate.

**Local Evaluator, Remote Log.** Z3 evaluates in-process (fast), proof entries ship to a remote store. Evaluations are synchronous, storage is async.

**Remote Evaluator (Non-Blocking).** The transport captures the frame snapshot and ships it to a remote evaluator. The application never blocks. Violations surface after the fact.

The modes are not mutually exclusive. A log statement in a payment path might be a local gate. A log statement in a background job might ship to a remote evaluator. Configuration determines the mode per call site.

## Output and Transport

The proof log is as pluggable as any logging framework. neurallog doesn't own the output — it produces structured proof entries and sends them wherever you want.

### Proof Entry Format

Every evaluation produces a structured proof entry:

```
{
  call_site: "src/billing.py:47",
  claim: "balance should remain non-negative after withdrawal",
  predicate: "(>= balance 0.0)",
  result: "pass" | "fail",
  certificate: <Z3 proof certificate>,
  values: { balance: 142.50, withdrawal: 30.00 },
  timestamp: "2026-04-14T03:22:41Z",
  contract_version: "a3f8c...",
  file_hash: "e7b2d..."
}
```

### Transports

Proof entries flow through pluggable transports, just like any logging framework:

- **stdout** — structured JSON lines, pipe them wherever you want
- **File** — append-only local proof log
- **HTTP** — POST to a remote proof store, SIEM, or observability platform
- **Syslog** — drop into existing log infrastructure
- **Message queue** — Kafka, NATS, Redis streams — for high-throughput production
- **Custom** — implement a transport interface, send proof entries anywhere

Multiple transports can be active simultaneously. The same proof entry can go to stdout for local debugging AND to Kafka for production aggregation.

### Existing Log Framework Integration

neurallog doesn't replace your logging pipeline — it augments it. Proof entries are structured data. They can be formatted as:

- Standard log lines (for human consumption)
- Structured JSON (for machine consumption)
- OpenTelemetry spans (for tracing integration)
- Custom formats (for whatever your infrastructure expects)

A proof entry that passes is a verified log line. A proof entry that fails is an alert. The downstream system decides how to handle it — neurallog just produces the evidence.

## Cross-File Proof Chains

Contracts don't exist in isolation. The postcondition of a contract in one file becomes the precondition of a contract in another. Proofs chain across file boundaries. Z3 verifies the entire chain.

What neurallog actually proves are **Hoare triples**: `{P} code {Q}`. The log statements give us the locations. The LLM derives preconditions (P), postconditions (Q), and transition relations (what the code between log statements does). Z3 proves that P + transition → Q.

### Call-Site Binding (How Cross-File Composition Actually Works)

A postcondition in one file doesn't magically become a precondition in another. Variables are bound at the call site. The composition mechanism is explicit:

When `pricing.py` calls `stock = inventory.check_availability(product_id)`, the call site creates a **binding**: the return value of `check_availability` is bound to the local variable `stock` in `pricing.py`. The postcondition of `check_availability` (`return_value >= 0`) is carried across the file boundary through this binding to become `stock >= 0` in `pricing.py`.

This is not name matching. It is call-site binding — the same mechanism programming languages use for argument passing and return values. The AST gives us:
- The call expression: `inventory.check_availability(product_id)`
- The assignment target: `stock`
- The callee's contract: postcondition on return value

The composition rule: for each call expression `result = callee(args)`:
1. The callee's postconditions on its return value bind to `result`
2. The callee's postconditions on its parameters bind to the corresponding `args`
3. The callee's side effects are applied to the shared state model

When variables are renamed across boundaries — `stock` in one file, `available_qty` in another — the binding is through the call site, not through the name. The AST resolves this mechanically.

**Interfaces and dynamic dispatch.** When the callee is an interface rather than a concrete function, composition uses the interface's contract, not any specific implementation's. Implementations must satisfy the interface contract (a separate check). The caller composes against the interface — the weakest guaranteed behavior. This is standard in refinement type systems and is the only sound approach for dynamic dispatch.

**Higher-order functions and callbacks.** Call-site binding handles `result = callee(args)` where the callee is statically resolvable. It does not handle: `map(fn, items)` where `fn` is a variable, stored callbacks (`self.handler = ...` assigned at init and called later), decorators, or any case where the callee identity depends on runtime state. In v1, neurallog does not verify across higher-order boundaries. If the callee cannot be statically resolved from the AST, the call site is treated as opaque — no cross-file composition, no precondition propagation. The contract for the calling function notes the unresolved call as a gap. This is an explicit scope limitation, not an oversight. Future versions may support higher-order contracts (function types with pre/postconditions, as in refinement type systems), but v1 is honest about what it can't resolve.

**Database reads.** A value read from a database has no contract from the code — it's whatever was committed by whatever transaction under whatever isolation level. The postcondition of a DB read is: "this value was the state of this row at the time of this read, under this isolation level." Cross-function proofs that depend on DB-read values are only valid if the reads occur within the same transaction at a sufficient isolation level. This is where the concurrency axioms (below) become essential.

### Example: E-Commerce Order Processing

Three files, each calls the others, each has ordinary log statements:

```python
# ============================================
# inventory.py — stock management
# ============================================

def check_availability(product_id):
    available = db.get_stock(product_id)
    logger.info(f"Stock check for {product_id}: {available} available")
    return available

def reserve_stock(product_id, quantity):
    available = db.get_available(product_id)
    reserved = db.get_reserved(product_id)
    total = available + reserved
    logger.info(f"Reserving {quantity} of {product_id} ({available} available, {reserved} reserved)")

    db.set_available(product_id, available - quantity)
    db.set_reserved(product_id, reserved + quantity)

    new_available = available - quantity
    new_reserved = reserved + quantity
    logger.info(f"Reservation complete: {new_available} available, {new_reserved} reserved")
    logger.debug(f"Stock total: {new_available + new_reserved}")
```

```python
# ============================================
# pricing.py — price calculation
# ============================================

def calculate_price(product_id, base_price, quantity):
    stock = inventory.check_availability(product_id)
    logger.debug(f"Pricing {product_id} with stock level {stock}")

    scarcity_factor = 1.0 + (1.0 / (stock + 1))
    unit_price = base_price * scarcity_factor
    logger.info(f"Unit price for {product_id}: {unit_price} (base: {base_price}, scarcity: {scarcity_factor})")

    total = unit_price * quantity
    logger.info(f"Line total: {total} ({quantity} x {unit_price})")
    return total
```

```python
# ============================================
# orders.py — order orchestration
# ============================================

def place_order(customer, cart):
    line_totals = []
    for item in cart:
        price = pricing.calculate_price(
            item.product_id, item.base_price, item.quantity
        )
        line_totals.append(price)

    order_total = sum(line_totals)
    logger.info(f"Order total for {customer.id}: ${order_total:.2f}")

    for item in cart:
        inventory.reserve_stock(item.product_id, item.quantity)

    logger.info(f"Order placed for {customer.id}, {len(cart)} items reserved")
```

Nine log statements. Nine intent signals. The programmers were just logging values and events. neurallog reads the code and derives what should be true:

### Proof 1: Stock Conservation (inventory.py, self-contained)

From `logger.info(f"Reservation complete: {new_available} available, {new_reserved} reserved")` and `logger.debug(f"Stock total: {new_available + new_reserved}")`, the LLM reads the surrounding code and derives:

- Pre: `available >= 0 AND reserved >= 0 AND quantity > 0 AND quantity <= available`
- Transition: `new_available = available - quantity`, `new_reserved = reserved + quantity`
- Post: `new_available >= 0 AND (new_available + new_reserved) = (available + reserved)`

```smt2
(declare-const available Real)
(declare-const reserved Real)
(declare-const quantity Real)
(declare-const new_available Real)
(declare-const new_reserved Real)

; Preconditions
(assert (>= available 0))
(assert (>= reserved 0))
(assert (> quantity 0))
(assert (<= quantity available))

; Transition — the code between the log statements
(assert (= new_available (- available quantity)))
(assert (= new_reserved (+ reserved quantity)))

; Negate postconditions
(assert (or
  (< new_available 0)
  (not (= (+ new_available new_reserved) (+ available reserved)))
))

(check-sat)
; unsat → PROVEN: for ANY valid reservation, stock never goes negative
;         AND total is always conserved. Not just today. Always.
```

The programmer just logged values. The system proved a conservation law.

### Proof 2: Scarcity Pricing Bounded (pricing.py → inventory.py, cross-file)

From `logger.debug(f"Pricing {product_id} with stock level {stock}")`, the LLM follows the call into `inventory.check_availability()`. It picks up inventory's postcondition (`stock >= 0`) and carries it into pricing as a precondition:

```smt2
(declare-const stock Real)
(declare-const scarcity_factor Real)

; Precondition — carried from inventory.py's contract
(assert (>= stock 0))

; Transition — the scarcity formula
(assert (= scarcity_factor (+ 1.0 (/ 1.0 (+ stock 1.0)))))

; Negate postcondition
(assert (or (< scarcity_factor 1.0) (> scarcity_factor 2.0)))

(check-sat)
; unsat → PROVEN: scarcity factor always in [1.0, 2.0]
;         given stock >= 0 (guaranteed by inventory.py's contract)
```

The programmer logged a stock level. The system proved the pricing formula is bounded.

### Proof 3: Line Total Correctness (pricing.py, chained from Proof 2)

```smt2
(declare-const base_price Real)
(declare-const scarcity_factor Real)
(declare-const quantity Real)
(declare-const unit_price Real)
(declare-const total Real)

; Preconditions
(assert (> base_price 0))
(assert (> quantity 0))
; Carried from Proof 2
(assert (>= scarcity_factor 1.0))
(assert (<= scarcity_factor 2.0))

; Transitions
(assert (= unit_price (* base_price scarcity_factor)))
(assert (= total (* unit_price quantity)))

; Negate
(assert (or
  (<= unit_price 0)
  (> unit_price (* 2.0 base_price))
  (<= total 0)
))

(check-sat)
; unsat → PROVEN: unit price in (0, 2*base], total always positive
```

### Proof 4: Order Total (orders.py → pricing.py, cross-file)

```smt2
(declare-const line_1 Real)
(declare-const line_2 Real)
(declare-const order_total Real)

; Preconditions — carried from pricing.py's contracts
(assert (> line_1 0))
(assert (> line_2 0))

; Transition
(assert (= order_total (+ line_1 line_2)))

; Negate
(assert (<= order_total 0))

(check-sat)
; unsat → PROVEN: order total always positive if line items are positive
```

### Proof 5: The Bug (orders.py → inventory.py, cross-file)

`orders.py` calls `inventory.reserve_stock()`. The precondition of `reserve_stock` requires `quantity <= available`. But `orders.py` never checks availability before reserving:

```smt2
(declare-const quantity Int)
(declare-const available Int)

; What orders.py guarantees: quantity > 0 (from cart)
(assert (> quantity 0))

; reserve_stock REQUIRES: quantity <= available
; orders.py provides NO constraint on available
; Can we violate the precondition?
(assert (> quantity available))

(check-sat)
; SAT → model: quantity = 5, available = 3
; BUG FOUND: orders.py can call reserve_stock with insufficient stock
```

Z3 returns **sat** with a counterexample. The proof chain breaks at the file boundary. The programmer logged "Order placed, items reserved" — the system traced the call into `inventory.py` and proved the code can't guarantee it.

### Proof Dependency Graph

```
inventory.py                pricing.py               orders.py
─────────────              ────────────             ──────────

[stock >= 0] ─────────────→ [scarcity ∈ [1,2]]
                                    │
                             [unit_price > 0]
                             [total > 0] ──────────→ [order_total > 0] ✓

[quantity <= available] ◄── ── ── ── ── ── ── ── ── [??? NOTHING] ✗
        ↓                                           BUG: precondition
[stock >= 0 after] ✓                                not established
[conservation] ✓                                    by caller
```

### Latent Bug Discovery: One New Log Statement Breaks Everything

This is the most powerful property of cross-file proof chains. A system can be green — all invariants consistent, all proofs passing — and then a single new log statement reveals a bug that was always there.

**Setup:** The e-commerce system above has been running for months. Proofs pass. A developer adds a refund path:

```python
# inventory.py — release_stock added for refunds
def release_stock(product_id, quantity):
    available = db.get_available(product_id)
    reserved = db.get_reserved(product_id)
    logger.info(f"Releasing {quantity} of {product_id} back to stock")

    db.set_available(product_id, available + quantity)
    db.set_reserved(product_id, reserved - quantity)

    logger.info(f"Release complete: {available + quantity} available, {reserved - quantity} reserved")
```

```python
# orders.py — refund path added
def process_refund(order):
    refund_amount = pricing.calculate_refund(order)
    for item in order.items:
        inventory.release_stock(item.product_id, item.quantity)
    order.status = "refunded"
```

The refund path in `orders.py` has no log statements. No intent signals. neurallog doesn't analyze it. The invariants inside `release_stock` pass in isolation. Everything is green.

Months pass. During an incident investigation, a developer adds one line:

```python
def process_refund(order):
    refund_amount = pricing.calculate_refund(order)
    for item in order.items:
        inventory.release_stock(item.product_id, item.quantity)
    order.status = "refunded"
    logger.info(f"Refund processed for order {order.id}: ${refund_amount:.2f}")  # ← just logging
```

The system wakes up. The discovery agent traces the refund path. It follows `process_refund` → `release_stock`. It examines all callers of `process_refund`. It discovers: **there is no guard against double refunds**. `process_refund` can be called twice for the same order. The second call tries to release stock that was already released — reserved is now 0.

```smt2
(declare-const reserved_initial Int)
(declare-const quantity Int)
(declare-const reserved_after_first Int)
(declare-const reserved_after_second Int)

; Initial state: valid reservation exists
(assert (> reserved_initial 0))
(assert (> quantity 0))
(assert (<= quantity reserved_initial))

; First refund — release stock (all good)
(assert (= reserved_after_first (- reserved_initial quantity)))
(assert (>= reserved_after_first 0))  ; holds ✓

; Second refund — same order, same quantity, no guard
(assert (= reserved_after_second (- reserved_after_first quantity)))

; Can reserved go negative?
(assert (< reserved_after_second 0))

(check-sat)
; SAT → model: reserved_initial=5, quantity=5,
;              after_first=0, after_second=-5
; BUG: double refund drives reserved stock to -5
```

Z3 returns sat. A double refund on 5 units drives reserved stock to -5. This violates `inventory.py`'s contract.

**The bug was always there.** The code never prevented double refunds. But without an intent signal on the refund path, the system had no reason to look. One ordinary log statement — `logger.info(f"Refund processed for order {order.id}")` — was enough. The programmer just wanted to see refunds in the logs. The system proved their refund path is unsafe.

The fix: check `order.status != "refunded"` before processing. The LLM derives this from the unsat core — the contradiction is between "refund can happen twice" and "reserved can't go negative." The resolution is to prevent the second refund.

```python
def process_refund(order):
    if order.status == "refunded":
        logger.warn(f"Duplicate refund attempt for order {order.id}")
        return
    refund_amount = pricing.calculate_refund(order)
    for item in order.items:
        inventory.release_stock(item.product_id, item.quantity)
    order.status = "refunded"
    logger.info(f"Refund processed for order {order.id}: ${refund_amount:.2f}")
```

The LLM writes the patch. Z3 re-verifies. The proof chain holds. The PR is filed.

One log line. A latent bug found. A formally verified fix proposed.

## Retrofit: One Line to Formally Verified

neurallog doesn't require a greenfield codebase. It doesn't require rewriting your log statements. It doesn't require adopting a new framework.

You add one line.

```python
# Python
import neurallog  # that's it
```

```javascript
// Node.js
require('neurallog')  // that's it
```

```go
// Go
import _ "neurallog"  // that's it
```

The shim hooks your existing logging framework — Python's `logging`, Go's `slog`, Node's `console`, Java's `slf4j`, whatever you use. Every existing log call site in your codebase becomes an intent signal. Every `logger.info("processing order")` that a developer wrote three years ago and forgot about is now a candidate for formal verification.

An enterprise codebase with thousands of log statements becomes a candidate for formal verification. Overnight. Without changing a single line of application code.

### Honest Day-One Expectations

Not every log statement produces a strong contract on day one. The verification depth depends on the code:

**Pure computation code** (no DB, no I/O, no callbacks) gets strong contracts immediately. Arithmetic, data transformation, business logic — the LLM reads the code, Z3 proves properties. These are the easy wins.

**DB-touching code** gets weak contracts under v1's conservative defaults. P9 treats reads without explicit transaction annotations as `unknown` isolation, which means postconditions from DB reads don't propagate across function boundaries. Most existing applications have zero transaction annotations. The honest retrofit story for DB-heavy code: "you get local invariants per function, but cross-function proofs involving DB reads don't compose until you annotate your transaction boundaries." This is the right behavior — the system refuses to prove things it can't prove — but it means the "formally verified" pitch is partial until the codebase adds isolation annotations.

**Callback-heavy code** (JS/TS event handlers, stored functions, `map(fn, items)`) has opaque call sites that don't compose. In a typical JS/TS codebase, 30-50% of call sites may be higher-order. v1 treats these as gaps. The system reports them as unverified, not as verified-correct.

The pitch is not "formally verified overnight." The pitch is: "overnight, you know exactly which parts of your codebase are formally verified, which parts have weak contracts, and which parts are opaque. You've never had that map before. Now you can see where to invest."

### The Convergence Loop

The LLM doesn't rest.

On first deployment, every log call site triggers a cache miss. Contracts are derived lazily as the code executes. As traffic flows through the application, the contract cache fills. Z3 checks contracts for cross-path consistency. Contradictions are found. The LLM resolves them — fixing contracts that over-constrained, flagging code that violates programmer intent.

This isn't a one-shot analysis. It's a continuous convergence loop:

1. **Discovery** — log calls fire, contracts are derived for new call sites
2. **Consistency** — Z3 checks contract sets across code paths for mutual satisfiability
3. **Resolution** — contradictions are fed back to the LLM. Bad contracts are re-derived. Bad code is flagged with patches.
4. **Verification** — runtime values are checked against contracts. Violations are recorded with formal proofs.
5. **Repeat** — code changes invalidate contracts (MD5 mismatch), triggering re-derivation. New log statements trigger new contracts. The system never stops converging.

The system converges toward a state where **all invariants are both solvable** (contracts are mutually consistent) **and solved** (runtime values satisfy every contract). That's the steady state: a continuously, formally proven codebase.

## Production Failure Response

When an invariant breaks in production, the system has everything it needs to act:

- The **claim** — what the LLM derived should be true at this point
- The **proof of violation** — Z3's formal certificate of exactly what went wrong
- The **unsat core** — which specific constraints conflicted
- The **values** — the actual runtime state from the stack frame
- The **code context** — what the discovery agent found when deriving the contract
- The **contract provenance** — the full derivation chain

This isn't a stack trace. This is a complete, formally grounded bug report.

### Automated Response

When a violation is detected in production:

1. **File the bug** — automatically, with the claim as the title, the proof as the body, the values as reproduction context. Not "NullPointerException at line 47." Instead: *"balance should remain non-negative after withdrawal" — violated. balance was -32.50, expected >= 0. Contract derived from src/billing.py:47, withdrawal logic at src/billing.py:31-52.*

2. **Kick off the fix** — the violation, the proof, the code context, and the contract are handed to a coding agent. "Here's the invariant. Here's the proof that it was violated. Here's the code. Fix it." The agent has everything: the formal specification (the contract), the evidence (the proof), and the code. It produces a patch.

3. **Verify the fix** — the patch is applied in a sandbox. The contract is re-evaluated. Z3 confirms the invariant holds. The fix is verified before it's ever proposed to a human.

4. **Propose the PR** — a pull request with: the bug (formal proof of violation), the fix (LLM-generated patch), and the verification (Z3 confirmation that the invariant now holds). A human reviews, but the hard work is done.

The loop from "invariant violated in production" to "verified fix in a PR" is fully automated. A programmer's ordinary log statement started a chain that ended with a formally verified bugfix.

### The Full Lifecycle

```
Programmer writes: logger.info(f"Order total: {total}")
    |
    v
System derives what should be true here (contract)
    |
    v
System proves it holds (runtime verification)
    |
    v
System detects when it stops holding (violation)
    |
    v
System files the bug (with formal proof)
    |
    v
System writes the fix (with full context)
    |
    v
System verifies the fix (Z3 confirmation)
    |
    v
System proposes the PR (human reviews)
    |
    v
Programmer wrote a log line. Got a formally verified bugfix.
```

## Adapter Chain

neurallog produces proof entries. Transports put them places. **Adapters** watch those places and act. Each link is independent. neurallog doesn't know about any of them.

```
logger.info(f"Order total: {total}")
    |
    v
proof entry (pass or fail, with formal certificate)
    |
    v
transport: wherever your logs already go
    |
    v
adapter: watches for violations
    |
    v
adapter: violation → issue tracker
    (title: the claim, body: the proof, values, code context)
    |
    v
adapter: issue → coding agent
    (full context: invariant, violation, proof, code)
    |
    v
adapter: coding agent → verified PR
    (patch + Z3 confirmation that invariant now holds)
```

Each adapter is a simple, independent service. The proof entry format is the contract between them. Anyone can write an adapter for their stack — log aggregators, issue trackers, alerting systems, chat, compliance tooling, data warehouses.

The adapters don't need to understand Z3 or formal verification. They consume structured proof entries — JSON with a claim, a result, values, and a certificate. The formal verification is already done. The adapters just route and act.

neurallog is not a platform. It's a proof engine with a pluggable output. The ecosystem is the adapters. The value compounds as more adapters exist, but neurallog itself stays simple: derive contracts, evaluate them, emit proof entries.

## Proof Format

Every proof is self-contained. The SMT-LIB input is included in full. Anyone can copy it, run `z3 -in < file.smt2`, and independently verify the result. No trust required. This is the core guarantee: it's not "trust our system," it's "verify it yourself."

There are three kinds of proof in the system:

### Runtime Proofs (Specific Values)

The simplest kind. Plug in actual values from the stack frame, prove the invariant holds for this specific execution.

```json
{
  "type": "runtime",
  "call_site": "src/inventory.py:15",
  "function": "check_availability",
  "claim": "available stock count is non-negative",
  "values": { "available": 47 },
  "result": "pass",
  "timestamp": "2026-04-14T03:22:41Z",
  "smt2": [
    "(set-option :produce-proofs true)",
    "(declare-const available Int)",
    "",
    "; Concrete values from stack frame",
    "(assert (= available 47))",
    "",
    "; Negate the invariant (prove violation is impossible)",
    "(assert (not (>= available 0)))",
    "",
    "(check-sat)"
  ],
  "z3_result": "unsat",
  "proof": "(proof (mp (asserted (= available 47)) (asserted (not (>= available 0))) (th-lemma arith (not (and (= available 47) (not (>= available 0)))))))"
}
```

Z3 returns **unsat** — the negation of the invariant is impossible given the actual values. The invariant holds. The proof tree shows the derivation: a machine-checkable chain of inference steps.

This proves: at this moment, with these values, the invariant held.

When an invariant is **violated**, the runtime proof looks different:

```json
{
  "type": "runtime",
  "call_site": "src/billing.py:47",
  "function": "process_withdrawal",
  "claim": "balance remains non-negative after withdrawal",
  "values": { "balance": -32.50, "withdrawal": 75.00, "previous_balance": 42.50 },
  "result": "fail",
  "timestamp": "2026-04-14T03:22:41Z",
  "smt2": [
    "(set-option :produce-proofs true)",
    "(declare-const balance Real)",
    "",
    "(assert (= balance (- 32.50)))",
    "(assert (not (>= balance 0.0)))",
    "",
    "(check-sat)"
  ],
  "z3_result": "sat",
  "model": { "balance": -32.50 },
  "unsat_core": null
}
```

Z3 returns **sat** — it found values where the invariant is violated. The model shows exactly what went wrong. This is the formal proof of failure: not a stack trace, but a mathematical certificate that the invariant was violated, with the exact values.

### Static Proofs (Hoare Triples)

The powerful kind. Proves the invariant holds for **all possible values**, not just today's. These are the cross-code-path proofs that verify: if preconditions hold and the code does what we modeled, the postcondition must hold.

```json
{
  "type": "static",
  "scope": "src/inventory.py:reserve_stock",
  "claim": "stock conservation: available + reserved is invariant across reservation, and stock never goes negative",
  "preconditions": [
    "available >= 0",
    "reserved >= 0",
    "quantity > 0",
    "quantity <= available"
  ],
  "transition": [
    "new_available = available - quantity",
    "new_reserved = reserved + quantity"
  ],
  "postconditions": [
    "new_available >= 0",
    "(new_available + new_reserved) = (available + reserved)"
  ],
  "smt2": [
    "(set-option :produce-proofs true)",
    "(declare-const available Int)",
    "(declare-const reserved Int)",
    "(declare-const quantity Int)",
    "(declare-const new_available Int)",
    "(declare-const new_reserved Int)",
    "",
    "; Preconditions",
    "(assert (>= available 0))",
    "(assert (>= reserved 0))",
    "(assert (> quantity 0))",
    "(assert (<= quantity available))",
    "",
    "; Transition — what the code does",
    "(assert (= new_available (- available quantity)))",
    "(assert (= new_reserved (+ reserved quantity)))",
    "",
    "; Negate postconditions — prove violation is impossible",
    "(assert (or",
    "  (< new_available 0)",
    "  (not (= (+ new_available new_reserved) (+ available reserved)))))",
    "",
    "(check-sat)"
  ],
  "z3_result": "unsat",
  "proof": "(proof (mp ...))"
}
```

Z3 returns **unsat** — there is no possible combination of values satisfying the preconditions where the postcondition fails. This isn't a test that passed. It's a mathematical proof that the property holds universally.

This proves: for ANY valid reservation — any available count, any reserved count, any quantity — stock never goes negative and the total is always conserved. Forever.

### Cross-File Static Proofs

When the discovery agent follows a call chain across files, the postcondition of one file becomes the precondition of another. The static proof spans both:

```json
{
  "type": "static",
  "scope": "pricing.py:calculate_price → inventory.py:check_availability",
  "claim": "scarcity pricing factor is bounded between 1.0 and 2.0",
  "chain": [
    {
      "source": "src/inventory.py:15",
      "provides": "stock >= 0"
    },
    {
      "source": "src/pricing.py:8",
      "requires": "stock >= 0",
      "proves": "scarcity_factor >= 1.0 AND scarcity_factor <= 2.0"
    }
  ],
  "smt2": [
    "(set-option :produce-proofs true)",
    "(declare-const stock Real)",
    "(declare-const scarcity_factor Real)",
    "",
    "; Precondition — from inventory.py contract",
    "(assert (>= stock 0))",
    "",
    "; Transition — scarcity formula in pricing.py",
    "(assert (= scarcity_factor (+ 1.0 (/ 1.0 (+ stock 1.0)))))",
    "",
    "; Negate postcondition",
    "(assert (or (< scarcity_factor 1.0) (> scarcity_factor 2.0)))",
    "",
    "(check-sat)"
  ],
  "z3_result": "unsat",
  "proof": "(proof (mp ...))"
}
```

The `chain` field makes the dependency explicit: this proof is only valid because `inventory.py` guarantees `stock >= 0`. If inventory's contract is invalidated, this proof is also invalidated.

### Consistency Proofs

Verify that a set of contracts can all be true simultaneously. Here, **sat** is the good result — it means the contracts don't contradict each other.

```json
{
  "type": "consistency",
  "scope": "src/inventory.py",
  "contracts": [
    { "line": 15, "predicate": "(>= available 0)" },
    { "line": 25, "predicate": "(and (> quantity 0) (<= quantity available) (>= reserved 0))" }
  ],
  "claim": "contracts within inventory.py are mutually satisfiable",
  "smt2": [
    "(declare-const available Int)",
    "(declare-const quantity Int)",
    "(declare-const reserved Int)",
    "",
    "; Contract at line 15",
    "(assert (>= available 0))",
    "",
    "; Contract at line 25",
    "(assert (> quantity 0))",
    "(assert (<= quantity available))",
    "(assert (>= reserved 0))",
    "",
    "(check-sat)"
  ],
  "z3_result": "sat",
  "model": { "available": 10, "quantity": 5, "reserved": 3 }
}
```

Z3 returns **sat** with a model: concrete values where all contracts hold simultaneously. The contracts can coexist.

When consistency fails:

```json
{
  "type": "consistency",
  "scope": "cross-file: orders.py → inventory.py",
  "contracts": [
    { "file": "src/orders.py", "line": 38, "predicate": "(> quantity 0)" },
    { "file": "src/inventory.py", "line": 25, "predicate": "(<= quantity available)" }
  ],
  "claim": "orders.py establishes inventory.py's precondition before calling reserve_stock",
  "smt2": [
    "(declare-const quantity Int)",
    "(declare-const available Int)",
    "",
    "; What orders.py guarantees",
    "(assert (> quantity 0))",
    "",
    "; What inventory.py requires",
    "(assert (not (<= quantity available)))",
    "",
    "(check-sat)"
  ],
  "z3_result": "sat",
  "model": { "quantity": 5, "available": 3 }
}
```

Z3 returns **sat** — it found values where the precondition is violated. `quantity = 5, available = 3`. The caller doesn't establish the callee's precondition. Bug found, with a concrete counterexample.

### Proof Verification

Every proof file is self-contained and independently verifiable. The `smt2` field is the complete Z3 input. To verify any proof:

```bash
echo '<smt2 content>' | z3 -in
```

Same result every time. No neurallog installation needed. No LLM needed. No network needed. Just Z3 and the proof file.

This means:
- **CI can verify proofs** without running neurallog — just Z3
- **Auditors can verify proofs** independently — just Z3
- **Other tools can consume proofs** — the format is standard SMT-LIB
- **Proofs can't be faked** — Z3 is deterministic; the input produces the output or it doesn't

## Proof Storage

Proofs live in the repo. They are version-controlled artifacts, committed alongside the code they prove.

### `.neurallog/` Directory

```
.neurallog/
  contracts/
    src/
      billing.py.json       # all contracts for billing.py
      auth.py.json           # all contracts for auth.py
      orders.py.json
  consistency/
    src/
      billing.py.proof       # internal consistency for billing.py contracts
    cross/
      billing-auth.proof     # cross-file consistency proofs
  cache.json                 # file hashes for staleness detection
```

One contract file per source file. All contracts for a file live together. When a source file changes, one contract file re-derives. Clean diff in PRs — you see the source change and the contract change side by side. `git blame` tells you when contracts were derived and what triggered re-derivation.

### Why the Repo

- **Diffable** — a contract changes, the proof changes, it's visible in the diff. You can see what your code was proving last week versus this week.
- **Reviewable** — contracts and proofs appear in PRs alongside the code changes that triggered them. Reviewers can see not just what changed, but what the system believes about the change.
- **CI verifiable** — a CI check can confirm that all contracts are satisfiable, all cross-path consistency checks pass, and no proofs are stale against the current file hashes.
- **Portable** — clone the repo, you have the proofs. No external service needed to know what your code guarantees.
- **Auditable** — the full history of what was proven, when, and against what code is in git history. Compliance gets a complete audit trail for free.

### Staleness in CI

CI runs can verify proof freshness:

1. Compute MD5 of every source file with contracts
2. Compare against hashes in `.neurallog/cache.json`
3. Any mismatch = stale proof. CI fails. Re-derive before merging.

This guarantees that every proof in the repo corresponds to the current state of the code.

### Merge Semantics for Clause History

`clause_history` tracks per-clause weaken steps and witness counts. Git merges can break the monotonicity assumption: branch A weakens clause X at step 3 with witness count 12, branch B weakens the same clause at step 5 with witness count 8. The merged result has a non-linear history.

The merge rule: for each clause present in both parents, take the max of `weaken_step` and the max of `witness_count_at_last_weaken` and `current_witness_count` across both parents. If the clauses themselves disagree (different SMT-LIB content for the same clause position), fall back to re-derivation from the merged file hash. This preserves monotonicity: the merged contract is at least as weakened and at least as witnessed as either parent.

## Trivial Invariants

Not every log statement guards something meaningful. `logger.debug("here")` has no variables in scope worth checking. `console.log("---")` is noise. The system handles this cleanly: the LLM derives an invariant of `true`.

`true` is always sat. The proof is trivial. The log call proceeds with zero overhead. No false violations. No noise. The system shrugs and moves on. The log statement still fires normally — it's still a logger.

The LLM has three possible outputs for any call site:

1. **Meaningful invariant** — something provable and useful. The system verifies it.
2. **`true`** — nothing to prove here. The system moves on. Zero cost.
3. **Wrong invariant** — the convergence loop catches this via unsat or runtime violations, and re-derives.

There is no failure mode that produces noise. Either you get signal or you get silence.

The contract map becomes a code quality signal for free: which log statements are doing real work (non-trivial invariants) and which are just noise. Over time, the ratio of meaningful invariants to trivial ones tells you something about the codebase.

## Self-Growing Verification Principles

The derivation prompt is not a static template. It is assembled from a growing library of verification principles — teaching examples that tell the LLM how to reason about code. The principles are the system's accumulated wisdom, distilled from every bug it has ever found.

### The Seed Set

The system ships with a seed set of principles drawn from formal verification:

1. **Precondition Propagation** — when A calls B, A must establish B's preconditions
2. **State Mutation Analysis** — mutations change the precondition landscape for subsequent calls; loop iterations sharing a resource via data-dependent identity (product_id, account_id) are not independent
3. **Calling Context Analysis** — public functions can receive any input; valid inputs are only what the function itself validates
4. **Temporal Analysis** — a function invoked twice on the same input may violate its own preconditions on the second call due to the first call's side effects
5. **Semantic Correctness** — a function may execute without error but produce a value that is meaningless in the domain
6. **Boundary and Degenerate Inputs** — empty collections, zero values, and single-element inputs can produce degenerate results that mask upstream bugs
7. **Arithmetic Safety** — division by zero, subtraction underflow, integer overflow

Each principle includes a description and one or more teaching examples in a different domain from any specific target code. The examples teach the LLM the verification *pattern*, not the specific bug.

### How Principles Grow

When the system finds a violation that doesn't match any existing principle:

1. **Detection.** Every violation is classified: "Does this fit an existing principle, or is it genuinely new?" This is an LLM call — compare the violation against the principle library.

2. **Generalization.** The LLM takes the specific violation and extracts the general pattern. "Refund exceeds payment because calculate_refund uses raw unit_price" becomes "a computed value diverges from the real-world state it represents because the computation uses stale or incomplete data."

3. **Teaching example generation.** The LLM creates a teaching example in a *different domain* — to avoid test leakage into the system's own prompt. The specific violation was about refunds; the teaching example uses tax computation or shipping costs.

4. **Self-validation.** Before committing, the system tests the new principle: add it to the prompt, re-derive contracts for a *different* file. Does the principle improve detection? If yes, it generalizes. If no, it's too specific — discard it.

5. **Commit.** The principle is written to `.neurallog/principles/`. Every future derivation benefits.

### Principle Storage

```
.neurallog/
  principles/
    01_precondition_propagation.md
    02_state_mutation.md
    03_calling_context.md
    04_temporal_analysis.md
    05_semantic_correctness.md
    06_boundary_inputs.md
    07_arithmetic_safety.md
    08_data_model_divergence.md      # system-generated
    09_concurrent_state_access.md    # system-generated
    ...
```

Each principle file contains:
- The principle name and description
- One or more teaching examples (domain, code sketch, SMT-LIB block, explanation)
- Provenance: which violation spawned it, which file, when, validation status
- Version number

Principles are version-controlled artifacts in the repo, like contracts. They show up in diffs. They're reviewable in PRs. They have full provenance.

### Principles Are Append-Only

Principles are never removed. A principle is a truth about software, not about a specific codebase. "State can change between reads" doesn't stop being true when you fix the TOCTOU bug in your code. It just stops applying *here*.

Contracts are mutable — they're derived, invalidated, re-derived, deleted. They track the current state of the code.

Principles are immutable — they represent general verification knowledge. They only grow.

Over time, related principles may be consolidated: five variants of "state changes between reads" merge into one richer principle. But the knowledge never shrinks.

### Prompt Assembly

At derivation time, the prompt is assembled dynamically:

1. **Base methodology** — Z3 verification patterns, SMT-LIB grammar, output format
2. **Relevant principles** — filtered by AST analysis of the target code (has loops? calls public functions? does arithmetic?) to keep prompt length manageable
3. **Existing contracts** — for the target file, its imports, and transitive deps
4. **Code context** — target file source, depth-1 import sources
5. **Target line** — the specific log statement being analyzed

As the principles directory grows, the prompt gets richer. Relevance filtering ensures the prompt stays focused. Tiered prompts use different subsets for different deployment modes (core principles for hot path, full set for deep analysis).

### Portability

Principles are language-agnostic. "Precondition propagation" applies to Python, Go, Rust, Java. A principle discovered analyzing a Python web app applies to a Go microservice.

This means principles are shareable. A company running neurallog across all their repos accumulates a shared principles library. Every codebase benefits from every other codebase's discoveries.

The principles directory could be published as an open-source library — a community-grown formal verification curriculum, built from real bugs in real code, that any neurallog installation can import.

**Multi-repo principle sharing (future work).** In v1, each repo has local `.neurallog/principles/`. A 50-repo company re-learns the same principle 50 times. The natural shape is a remote principle store — pull/push, like a package registry. `neurallog principles pull @company/shared` imports the company-wide principle library. `neurallog principles push` proposes locally-discovered principles to the shared store. Versioned, reviewed, signed. This is not in v1 scope but will come up in the first enterprise conversation. The architecture supports it — principles are portable JSON files with provenance — but the transport and governance are unspecified.

## Convergence

The system has two things converging simultaneously: contracts and principles. They interact.

### Contract Convergence (Per-Codebase)

Contracts converge toward a state where every contract is:
- Derived with the current principle set
- Consistent with all other contracts along the same code path
- Satisfied by runtime values

The cache key for a contract is:

```
file_path:line_number:md5(file_contents):md5(relevant_principles)
```

When a source file changes, the file hash changes, the contract is stale. When a principle is added, the principle hash changes, the contract is stale. Re-derivation happens lazily on next execution, or can be triggered by CI. Same invalidation mechanism for both — no special handling.

### Principle Convergence (Global, Append-Only)

Principles converge toward a comprehensive verification curriculum. The rate of new principle discovery decreases over time:

```
Codebases analyzed:    1    10    100    1000
New principles found:  7    15     25      30
```

The curve flattens as the principle library covers more bug classes. Early codebases generate many new principles. Later codebases mostly match existing patterns.

### Interaction Between Contract and Principle Convergence

A new principle can destabilize a previously-converged codebase:

1. New principle added → principle hash changes
2. All contracts are now stale (derived without this knowledge)
3. Re-derivation runs with the richer principle set
4. Previously-undetected violations surface
5. The unsat loop resolves them (fix contracts or fix code)
6. The codebase re-converges at a higher standard

This is like adding a new test to your test suite. Previously-passing code might fail. But the codebase is better for it.

### Contract Termination: Monotone Weakening

Principles are append-only, so the axiom set is monotonically increasing. But the original version of this spec hand-waved contract convergence by conflating principle monotonicity with contract stability. Principles don't oscillate; contracts absolutely can.

The failure mode: contracts A and B are mutually inconsistent. The unsat loop fires. The LLM re-derives. Under one prompt ordering it weakens A. Next time it weakens B. Then A again. The contracts cycle.

The fix is a tiebreaker rule that makes re-derivation a lattice descent with guaranteed termination:

**Monotone weakening only.** When resolving an unsat between contracts, the LLM is only allowed to *remove or weaken* clauses in the contract being re-derived. It may never add new preconditions or strengthen existing ones during resolution. Removing a clause makes the contract weaker (closer to `true`). The weakest possible contract is `true`. You can't go below `true`. Therefore the weakening sequence terminates.

Strengthening is a separate phase, gated on evidence:
- A contract can be strengthened only when new **runtime witnesses** provide evidence for a stronger invariant — concrete values that demonstrate a tighter bound holds in practice
- Strengthening requires Z3 confirmation that the stronger contract is consistent with all existing contracts
- Strengthening is never triggered by the unsat loop — only by positive evidence

This gives a two-phase convergence with provable termination:
1. **Weaken phase** (unsat resolution): monotone descent toward `true`. Terminates.
2. **Strengthen phase** (runtime evidence): monotone ascent toward the tightest consistent contract. Bounded by the contract lattice.

The interaction between phases requires a well-founded ordering to prevent cycles. The risk: strengthen adds a clause → new contract is unsat against B → weaken fires on the just-strengthened clause → strengthen re-adds it with more witnesses → loop.

The ordering that prevents this: each contract carries a pair `(distinct_witness_count, weaken_step_count)` ordered lexicographically.

- **Runtime witnesses are append-only.** Once a value is witnessed, the witness count never decreases. Strengthening is gated on strictly increasing witness counts — a clause can only be re-strengthened if new witnesses have arrived since it was last weakened.
- **Weaken steps are monotone.** Each weakening increments the weaken count. The LLM cannot re-add a clause that was weakened at step N unless the witness count has increased past what it was at step N.
- **The lex pair descends.** Either the witness count increases (and strengthening is justified by new evidence) or the weaken count increases (and the contract is strictly weaker). The pair `(witness_count, weaken_steps)` is well-founded under this lexicographic order. The sequence terminates.

This is not hand-waving. It is a concrete well-founded ordering. The two phases cannot cycle because strengthening requires strictly more evidence than the last weakening consumed.

**Source-locality rule.** When two contracts from different files contradict, the contract at the function's *definition site* outranks the contract at a *call site*. The definer knows its own invariants. Callers can't tighten callee contracts — they can only weaken their own assumptions. This prevents caller-induced churn.

**Provenance weight.** When weakening, prefer to preserve the contract with more runtime witnesses. A contract that has been confirmed by 10,000 runtime evaluations has more evidence behind it than one derived yesterday with no runtime data. The more-witnessed contract wins.

### Steady State

A codebase has converged when:
- All contracts are derived with the current principle set (no stale contracts)
- All cross-contract consistency checks pass (no contradictions)
- All runtime values satisfy their contracts (no violations)
- No new violations are producing new principles (principle set is stable for this codebase)

The proof log at steady state is: continuous, formally verified, comprehensive. Every log statement has a contract. Every contract is proven consistent. Every runtime value satisfies its contract. The codebase is formally verified — and the verification got there by itself, from ordinary log statements.

## Two-Layer Architecture: LLM Derives, Z3 Reasons

The principles are not just teaching examples for an LLM prompt. They are formal axioms — parameterizable Z3 check generators. This splits the system into two layers:

### Layer 1: Contract Derivation (LLM, Cold Path)

The LLM reads source code and derives atomic contracts per function:
- **Preconditions** — what the function requires
- **Postconditions** — what the function guarantees
- **Side effects** — what shared state the function mutates
- **Claims** — natural language descriptions (for humans only)

This happens once per function. Contracts are cached, keyed by file hash + principle hash. Re-derivation only occurs when code changes or new axioms are added. This is where the LLM's code comprehension is genuinely irreplaceable.

### Layer 2: Axiom Application (Z3, Hot Path)

Given the contracts from Layer 1, Z3 mechanically applies axioms to find violations. No LLM. No network. Microseconds.

Each axiom is a formal template that, given contracts for a caller and callee (or a loop and its body, or a public function and its parameters), generates a concrete Z3 check:

**P1 template (Precondition Propagation):**
```
For every call site where A calls B:
  For every precondition P in contract(B):
    Assert: what A guarantees at the call site
    Assert: NOT P
    check-sat → if sat, violation reachable
```

**P2 template (State Mutation in Loops):**
```
For every loop calling state-mutating function F:
  For every pair of iterations (i, j) that could share a resource key:
    Assert: F's postcondition from iteration i (state after mutation)
    Assert: NOT F's precondition for iteration j (using post-mutation state)
    check-sat → if sat, loop aliasing violation reachable
```

**P3 template (Calling Context):**
```
For every public function entry point:
  For every downstream precondition P reachable from this entry:
    Assert: (nothing — public functions guarantee nothing about inputs)
    Assert: NOT P
    check-sat → if sat, unvalidated input violation reachable
```

**P4 template (Temporal):**
```
For every function F that mutates state S:
  Assert: F's precondition holds for the first call
  Assert: F's postcondition (state after first call)
  Assert: NOT F's precondition (using post-first-call state)
  check-sat → if sat, double-invocation violation reachable
```

These checks are generated by template instantiation, not by an LLM reading code. The axiom template knows what to check. The contracts provide the values. Z3 does the reasoning.

### Example: Mechanical Violation Detection

Contracts already derived by the LLM:
```
reserve_stock:
  precondition: quantity > 0 AND quantity <= available AND available >= 0
  postcondition: new_available = available - quantity
  side_effect: mutates available, reserved for product_id
```

P1 template applied to `place_order` calling `reserve_stock`:

```smt2
; Mechanically generated — no LLM needed
; Source: P1 template × call site orders.py:13
; Callee precondition: quantity <= available
; Caller guarantees: quantity > 0 (from cart), nothing about available
(declare-const quantity Int)
(declare-const available Int)
(assert (> quantity 0))
(assert (not (<= quantity available)))
(check-sat)
; sat → violation reachable
```

P2 template applied to the reservation loop:

```smt2
; Mechanically generated — no LLM needed
; Source: P2 template × loop at orders.py:12-13
; Loop calls reserve_stock which mutates available[product_id]
; Two iterations may share a product_id
(declare-const available_initial Int)
(declare-const quantity_1 Int)
(declare-const quantity_2 Int)
(declare-const available_after_1 Int)
(assert (>= available_initial 0))
(assert (> quantity_1 0))
(assert (<= quantity_1 available_initial))
(assert (= available_after_1 (- available_initial quantity_1)))
(assert (> quantity_2 0))
(assert (not (<= quantity_2 available_after_1)))
(check-sat)
; sat → loop aliasing violation reachable
```

Neither check required an LLM. Both were generated mechanically from axiom templates + cached contracts.

### Performance Implications

```
First pass:   LLM derives contracts per function (slow, once)
After that:   Z3 applies axioms to cached contracts (microseconds, continuous)
Code change:  Re-derive contracts for changed functions only
New axiom:    Mechanically re-check ALL existing contracts (no LLM)
```

An enterprise codebase with 10,000 log statements:
- First pass: 10,000 LLM calls to derive function contracts. Slow. Once.
- Steady state: Axiom application is mechanical. Z3 checks all contract pairs against all axioms. Fast. Free. Continuous.
- New axiom discovered: mechanically applied to every existing contract. Finds violations that were always there but the old axiom set didn't cover. No LLM needed.

### The LLM's Diminishing Role

The LLM is needed for:
1. **Initial contract derivation** — reading code and understanding what functions do
2. **Semantic correctness (P5)** — domain-level reasoning about what values "should" mean
3. **Novel pattern discovery** — violations that don't match any axiom template
4. **Re-derivation** — when code changes invalidate cached contracts

The LLM is NOT needed for:
- Applying axioms to known contracts (Z3)
- Cross-contract consistency checking (Z3)
- Runtime value verification (Z3)
- Detecting violations from known axiom patterns (Z3)

Over time, as the contract store and axiom set grow, the LLM does less. Z3 does more. The system converges toward pure mechanical verification with LLM calls only at the frontier: new code, changed code, genuinely novel patterns.

### What This Really Is

This is Hoare logic with LLM-derived assertions. The axioms are inference rules. The contracts are assertions at program points. Z3 checks the proof obligations. The LLM is the oracle that provides the assertions the programmer was too busy to write.

The programmer writes log statements. The LLM derives formal contracts from the code around them. The axioms compose those contracts into proofs. Z3 verifies everything mechanically. The proof log records the results.

A logger that derives Hoare logic. That applies it mechanically. That fixes your code.

## Axiom Template Format

Each axiom has two representations:

1. **Teaching form** — natural language description + teaching examples in the derivation prompt. Used by the LLM in Phase 1.
2. **Formal form** — a parameterizable template that generates Z3 checks mechanically. Used by Layer 2 without the LLM.

The formal form is what makes the two-layer architecture work. It defines: what code pattern to match, what contract fields to read, and how to generate the Z3 check.

### Template Structure

```json
{
  "id": "P1",
  "name": "Precondition Propagation",

  "match": {
    "pattern": "function_call",
    "description": "Any call site where function A calls function B",
    "ast_selector": {
      "node_type": "call_expression",
      "resolve": "callee_definition"
    }
  },

  "requires": {
    "caller_contract": ["postconditions", "established_facts"],
    "callee_contract": ["preconditions"]
  },

  "generate": {
    "for_each": "callee.preconditions",
    "variables": {
      "from_caller": "caller.established_facts",
      "from_callee": "callee.preconditions[i]"
    },
    "smt2_template": [
      "; P1: Does {caller} establish {callee}'s precondition?",
      "; Call site: {call_site}",
      "; Precondition under test: {precondition_description}",
      "{declare_variables}",
      "{caller_guarantees}",
      "(assert (not {precondition}))",
      "(check-sat)",
      "; sat → caller does not establish the precondition"
    ]
  }
}
```

### The Seven Seed Axiom Templates

#### P1: Precondition Propagation

```
Match:    function_call(caller, callee, args)
Requires: callee.preconditions, caller.established_facts
For each: precondition P in callee.preconditions
Generate:
    assert(caller.established_facts)
    assert(NOT P)
    check-sat
    sat → caller fails to establish P
```

#### P2: State Mutation in Loops

```
Match:    loop containing call to state-mutating function F
Requires: F.preconditions, F.postconditions, F.side_effects
          F.side_effects.key_field (the identity of the mutated resource)
Generate:
    ; Model two iterations on the same resource
    declare iteration_1_state, iteration_2_state
    assert(F.preconditions hold for iteration 1)
    assert(F.postconditions define iteration_1_state)
    ; Alias: both iterations target the same resource
    assert(resource_key_1 = resource_key_2)
    ; Iteration 2 sees post-mutation state
    assert(iteration_2_input_state = iteration_1_output_state)
    assert(NOT F.preconditions for iteration 2)
    check-sat
    sat → loop aliasing violation reachable
```

#### P3: Calling Context

```
Match:    public function (no access restriction)
Requires: all downstream callee.preconditions reachable from this entry
Generate:
    ; Caller guarantees: NOTHING (public entry)
    for each downstream precondition P:
        assert(NOT P)
        check-sat
        sat → unvalidated input can violate P
```

#### P4: Temporal (Double Invocation)

```
Match:    function F that calls state-mutating function G
Requires: G.preconditions, G.postconditions, G.side_effects
          F has no idempotency guard (no status/flag check before G)
Generate:
    ; Model two invocations of F on the same input
    declare state_initial, state_after_first, state_after_second
    assert(G.preconditions hold for first call)
    assert(G.postconditions define state_after_first)
    ; Second call: same arguments, post-mutation state
    assert(state_after_second = G.transition(state_after_first))
    assert(NOT G.preconditions for second call using state_after_first)
    check-sat
    sat → double invocation violates G's preconditions
```

#### P5: Semantic Correctness

```
Match:    function that computes a return value or mutates domain state
Requires: domain_constraints (LLM-derived — what values are "meaningful")
Generate:
    ; This axiom is partially mechanical, partially LLM-dependent
    ; The domain constraints come from LLM analysis
    ; But once derived, the check is mechanical:
    assert(computation_model)
    assert(NOT domain_constraint)
    check-sat
    sat → semantically invalid output is reachable
```

P5 is the only axiom that cannot be fully mechanized. The domain constraints ("a refund should not exceed the payment") require LLM reasoning to derive. Once derived, the Z3 check is mechanical.

#### P6: Boundary and Degenerate Inputs

```
Match:    function that iterates a collection or accumulates a value
Requires: collection_source (parameter, DB query, etc.)
          accumulation_identity (what's the "zero" case — sum starts at 0, list starts empty)
Generate:
    ; Empty collection case
    assert(collection_length = 0)
    assert(result = identity_value)  ; sum([]) = 0, etc.
    check-sat
    sat → degenerate input produces identity result with no guard

    ; Zero-valued element case
    assert(collection_length > 0)
    assert(element_value = 0)
    assert(result = 0)  ; if multiplication, one zero kills the product
    check-sat
    sat → zero element nullifies computation
```

#### P7: Arithmetic Safety

```
Match:    arithmetic expression (division, subtraction)
Requires: operand constraints from upstream contracts
Generate:
    ; Division by zero
    for each division a / b:
        assert(upstream_constraints_on_b)
        assert(b = 0)
        check-sat
        sat → division by zero reachable

    ; Subtraction underflow (for non-negative domains)
    for each subtraction a - b where a should be >= 0:
        assert(upstream_constraints)
        assert(b > a)
        check-sat
        sat → underflow reachable
```

#### P8: Atomicity Boundary

Sequential single-threaded axioms (P1-P7) are insufficient for real services. The concurrency axioms extend the seed set to handle shared mutable state under concurrent access.

```
Match:    two or more reads of shared state (DB, cache, global) within
          one function, not wrapped in a transaction or lock
Requires: side_effects with matching key_field across the reads
          isolation_level annotation (if available)
Generate:
    ; Model a concurrent mutator between the two reads
    declare state_at_read_1, state_at_read_2
    ; Both reads target the same resource
    assert(key_field_1 = key_field_2)
    ; No atomicity guarantee: state can change between reads
    assert(NOT (state_at_read_1 = state_at_read_2))
    ; Can the function's postcondition be violated by the divergence?
    assert(postcondition using state_at_read_1 for first value
           and state_at_read_2 for second value)
    assert(NOT postcondition_holds)
    check-sat
    sat → TOCTOU / non-atomic observation violation reachable
```

#### P9: Isolation-Level-Aware Reads

```
Match:    DB read followed by logic that depends on the read value
Requires: isolation_level of the read (READ_COMMITTED, REPEATABLE_READ,
          SERIALIZABLE, or unknown)
          downstream preconditions that assume the read value is stable
Generate:
    ; Under READ_COMMITTED, the value can change after the read
    ; Postconditions from this read only propagate as preconditions
    ; to other functions if the isolation level is sufficient
    assert(isolation_level = READ_COMMITTED)
    ; Another transaction can commit a change to this row
    declare value_at_read, value_at_use
    assert(NOT (value_at_read = value_at_use))
    ; Can downstream preconditions be violated?
    assert(downstream_precondition using value_at_read)
    assert(NOT downstream_precondition using value_at_use)
    check-sat
    sat → read value is stale by the time it's used
```

**Where isolation_level comes from.** Most application code doesn't annotate isolation levels, and having the LLM guess them is exactly the kind of soft inference the two-layer architecture is supposed to eliminate. P9 handles this conservatively:

1. **Explicit transaction blocks.** If the AST shows a transaction context (`BEGIN`/`COMMIT`, `@transactional`, `with db.transaction():`, etc.), the isolation level is read from the transaction's configuration or defaults.
2. **ORM/framework annotations.** Common ORMs expose isolation level in decorators or configuration. The language adapter can extract these from known patterns.
3. **Default: unknown.** If no transaction context is detectable, isolation level is `unknown`. Under `unknown`, P9 treats the read as READ_COMMITTED (the weakest common default) — meaning the value is assumed stale-capable. Postconditions from `unknown` reads do not propagate as preconditions to downstream functions.

This is conservative by design. If you want reads to propagate, wrap them in an explicit transaction. P9 rewards code that is explicit about its isolation guarantees and penalizes code that isn't.

#### P10: Effect Causality

```
Match:    two async operations on the same key_field without
          an explicit ordering (await, .then, happens-before edge)
Requires: side_effects with matching key_field
          async/await annotations or callback structure from AST
Generate:
    ; Without explicit ordering, two effects are concurrent
    ; Model both orderings
    declare state_after_effect_1_then_2
    declare state_after_effect_2_then_1
    ; If both orderings produce different results, ordering matters
    ; but the code doesn't enforce one
    assert(NOT (state_after_effect_1_then_2 = state_after_effect_2_then_1))
    check-sat
    sat → race condition: outcome depends on uncontrolled ordering
```

The seed set is now 10 axioms: P1-P7 for sequential reasoning, P8-P10 for concurrent/async reasoning. The concurrency axioms make the system honest about what it can and can't prove under shared mutable state.

### How Templates Are Instantiated

The mechanical pipeline for Layer 2:

1. **AST scan.** Tree-sitter parses the source file. Identify all patterns that match axiom templates: call sites (P1), loops with state-mutating calls (P2), public functions (P3), functions calling state-mutators (P4), collection iterations (P6), arithmetic expressions (P7).

2. **Contract lookup.** For each matched pattern, read the relevant contracts from `.neurallog/contracts/`. The template's `requires` field specifies which contract fields are needed.

3. **Template instantiation.** Substitute the contract values into the template's `smt2_template`. This produces a concrete, self-contained SMT-LIB block.

4. **Z3 evaluation.** Feed each generated block to Z3. Record the result.

5. **Result handling.** `sat` results are violations — route to bug filing, fix generation, or alerting. `unsat` results are proofs — record in the proof store.

No LLM in this pipeline. It runs on every commit, every CI check, every deployment. It's the hot path.

### System-Generated Axiom Templates

When Phase 2 discovers a new principle and formalizes it (Step 2d), the output includes a template in this format. The template is stored in `.neurallog/principles/` alongside the teaching form.

The self-validation step (Step 2c) tests both forms: the teaching form is tested by adding it to the LLM prompt. The formal template is tested by running it mechanically against contracts for a different file. Both must produce useful results for the principle to be committed.

### Template Consistency Checking

Because axioms are formal, Z3 can verify the axiom set itself:

**Consistency:** Feed all axiom templates (instantiated with symbolic variables) to Z3 as a set. If `unsat`, two axioms contradict — the axiom system is inconsistent.

**Independence:** For each axiom A, check whether A's conclusions are derivable from the remaining axioms. If yes, A is redundant — it's a theorem, not an axiom.

**Coverage:** Given a set of contracts, count how many code patterns are matched by at least one axiom. Unmatched patterns represent gaps in the axiom system — areas where the LLM is the only line of defense.

## Security Model: Proofs, Not Data

The proof always runs local. Values never leave the machine. The proof does.

### No Raw Values by Default

Z3 evaluates contracts against live values *in the local process*. The output is a proof certificate — a mathematical object that proves "this property held" without revealing the underlying values. The verifier can check the proof without seeing the data.

| Artifact | Contains values? | Leaves the machine? |
|---|---|---|
| Stack frame capture | Yes — live values | No — evaluated locally, then discarded |
| Proof entry (default) | No — pass/fail + certificate only | Yes — shipped to transports, stored in proof log |
| Proof entry (debug mode) | Optionally yes — values attached | Configurable — can restrict to local only |
| Contract | No — predicates and claims, not data | Yes — committed to repo in `.neurallog/` |
| Axiom templates | No — abstract patterns | Yes — committed to repo |
| Consistency proofs | No — relations between contracts | Yes — committed to repo |

The default mode: **proofs only, no values.** A proof entry says "at src/billing.py:47, the property 'balance >= 0' held at 2026-04-14T03:22:41Z" with a Z3 certificate. It does not say what the balance was.

### What's Safe to Commit

Everything in `.neurallog/` is safe to commit to a public repository:
- **Contracts** contain predicates (`(>= balance 0)`) and claims ("balance is non-negative"). No values.
- **Proofs** contain SMT-LIB formulas and Z3 results. No values.
- **Principles** contain teaching examples from unrelated domains. No application data.
- **Cache** contains file hashes and principle hashes. No values.

### What's Safe to Ship

Proof entries shipped to external transports (log aggregators, monitoring, compliance systems) contain:
- Call site (file + line)
- Claim (natural language)
- Result (pass/fail)
- Certificate (Z3 proof)
- Timestamp
- Contract version and file hash

They do not contain runtime values unless the operator explicitly enables value inclusion. Value inclusion can be scoped: enabled for dev environments, disabled for production.

### Sensitive Code Paths

Some contracts might reveal business logic through their predicates — "price must be less than 2x base price" reveals pricing strategy. The contract is a formal description of what the code does.

For codebases where contract content itself is sensitive:
- Contracts can be excluded from version control (`.gitignore` the `.neurallog/contracts/` directory)
- Proof entries can omit the claim and predicate, shipping only the result (pass/fail) and a contract hash
- The proof is still verifiable locally — the full contract exists on the machine that generated it

### The Principle

The design principle is: proofs leave the machine, values don't. This is not zero-knowledge in the cryptographic sense — predicates and variable names do leak semantics. `(>= credit_score 620)` reveals a business rule. For codebases where contract content itself is sensitive, contracts can be excluded from version control or proof entries can omit predicates, shipping only pass/fail and a contract hash. But the default posture is: no raw runtime values in any artifact that leaves the process.

## The Verification Dial

neurallog is not an all-or-nothing proposition. It's a dial. Every setting is useful. Every setting degrades gracefully to the one below it.

### Level 0 — Stairs

It's a logger. Your log statements work exactly as before. neurallog is installed but passive. Nothing changes.

A broken escalator becomes stairs.

### Level 1 — Moving Sidewalk

The LLM derives contracts from your log statements and reports what it found. Advisory only. No enforcement. No Z3. Just insights.

"Your refund path has no idempotency guard. Your reservation loop doesn't check availability. Your pricing uses a stale stock read."

You're still walking. The sidewalk just helps you move faster.

### Level 2 — Escalator

Z3 verifies the contracts statically. `neurallog analyze src/` runs in CI. Reachable violations fail the build. Axiom templates apply mechanically to every contract pair. Your log statements are assertions that block merges.

You're formally verifying your code. You didn't write a single test.

### Level 3 — Elevator

Runtime verification. Live values checked against contracts. Proof log streaming. Production violations auto-file bugs, kick off the coding agent, propose verified PRs. The full convergence loop. Self-growing principles. The system teaches itself.

You pressed a button. The system does the rest.

### Degradation

Every level degrades to the one below it when a component fails:

| Component fails | Level 3 becomes | Level 2 becomes | Level 1 becomes |
|---|---|---|---|
| LLM unreachable | Level 2 (cached contracts still work) | Level 2 (axiom templates still work) | Level 0 (just a logger) |
| Z3 unavailable | Level 1 (advisory only) | Level 1 (advisory only) | Level 1 (advisory only) |
| Network down | Level 2 (local Z3 still works) | Level 2 (local Z3 still works) | Level 0 (just a logger) |
| Everything fails | Level 0 | Level 0 | Level 0 |

Level 0 doesn't fail. It's stairs.

### Adoption

```bash
npm install neurallog            # stairs
neurallog init --level 1         # moving sidewalk — see what it finds
neurallog init --level 2         # escalator — enforce in CI
neurallog init --level 3         # elevator — full runtime verification
```

Start at 1. See the insights. Gain trust. Turn it up.

## Implementation Roadmap

### Milestone 1: Stairs to Moving Sidewalk

**The CLI that analyzes a single TypeScript file.**

- Tree-sitter TypeScript parser: find log statements, extract enclosing functions
- LLM integration: send file + log statement to ollama, get contracts back
- Output: human-readable report of what the LLM found — claims, potential violations
- No Z3, no caching, no axiom templates. Just the LLM reading code and reporting.
- This is Level 1 for a single file. `neurallog analyze file.ts`

**Proves:** the LLM can derive meaningful invariants from real log statements in real code.

### Milestone 2: Moving Sidewalk with Z3

**Add Z3 verification to the output.**

- Z3 integration: validate every SMT-LIB block the LLM produces
- Proven properties (unsat) and reachable violations (sat) are distinguished
- False positives (LLM claimed a bug, Z3 says unsat) are automatically discarded
- Output: verified proofs and verified violations, not just LLM opinions

**Proves:** the LLM + Z3 combination produces formally verified results.

### Milestone 3: Multi-File Analysis

**Depth-1 import resolution and cross-file contracts.**

- TypeScript import resolution: follow `import`/`require` to source files
- Phase 0 context assembly: target file + imported function source + existing contracts
- Cross-file precondition propagation: caller doesn't establish callee's requirements
- `neurallog analyze src/` works on a directory

**Proves:** cross-file proof chains find bugs that single-file analysis misses.

### Milestone 4: Caching and CI

**Contract storage, staleness detection, CI integration.**

- Write contracts to `.neurallog/contracts/`
- Cache keyed by `file_hash:principle_hash`
- `neurallog analyze src/ --ci` exits non-zero on violations
- `neurallog analyze src/ --diff HEAD~1` only re-derives changed files
- This is Level 2. Log statements are now assertions that block merges.

**Proves:** the system is fast enough and reliable enough for CI.

### Milestone 5: Layer 2 — Axiom Template Engine

**Mechanical Z3 checks without the LLM.**

- Formalize the seven seed axioms as parameterizable templates
- AST scan + contract lookup + template instantiation → Z3 checks
- New axiom applied to existing contracts without re-derivation
- The hot path is now pure Z3. The LLM only runs on cache misses.

**Proves:** the two-layer architecture works. Performance scales.

### Milestone 6: Self-Growing Principles

**The system teaches itself.**

- Phase 2: classify `[NEW]` violations, generalize to teaching examples
- Self-validation: test new principles against different code
- Axiom template formalization for new principles
- Principle storage in `.neurallog/principles/`

**Proves:** the principle library grows from real bugs. The system gets smarter.

### Milestone 7: Runtime Mode

**The logging transport. Level 3.**

- TypeScript adapter: pino transport (or console proxy)
- Stack frame capture: V8 inspector or source transformation
- Runtime contract evaluation against live values
- Proof log streaming to configured transports
- Production violation → file bug → coding agent → verified PR

**Proves:** runtime formal verification from ordinary log statements.

### Milestone 8: Second Language

**Python adapter. Proves language neutrality.**

- tree-sitter-python for AST parsing
- Python logging handler for the runtime hook
- `inspect.currentframe().f_back.f_locals` for stack frame capture
- Python import resolution

**Proves:** the engine is language-neutral. Adding a language is just an adapter.

### What Ships When

| Milestone | Level | What you get |
|---|---|---|
| 1 | 1 (single file) | LLM reads your code, tells you what it found |
| 2 | 1 (verified) | Z3 validates, false positives discarded |
| 3 | 1 (multi-file) | Cross-file bugs found |
| 4 | 2 | CI integration, formal verification blocks merges |
| 5 | 2 (fast) | Mechanical verification, LLM only on cache miss |
| 6 | 2 (learning) | System discovers new verification patterns |
| 7 | 3 | Runtime proof log, production verification |
| 8 | 3 (multi-lang) | Python support, language-neutral proven |

## Why Not Refinement Types?

Formal verification of software exists. F*, Dafny, Liquid Haskell, and Lean can prove properties about code with zero LLM in the loop, zero convergence worries, zero "trust the model's SMT-LIB" risk. They're more rigorous than neurallog will ever be.

neurallog is not competing with refinement types. It's solving a different problem.

**Refinement types require you to write the types.** You annotate your code with formal specifications. You learn a new type system. You rewrite your functions to satisfy the checker. The payoff is enormous — but the cost is rewriting your codebase in a verification-aware language, or adding dense annotations to your existing one. Teams that do this (aerospace, cryptography, certain financial systems) get provably correct software. Teams that don't — which is nearly everyone — get nothing.

**neurallog requires you to have log statements.** You already do. Every codebase has thousands of them. neurallog reads them, derives contracts from the surrounding code, and formally verifies the contracts with Z3. The cost is `npm install neurallog`. The payoff is formal verification of whatever properties the LLM can derive — which is less than what a human expert with F* would produce, but infinitely more than what a team without formal verification has today.

The bet is: **partial formal verification of 100% of codebases beats total formal verification of 0.1% of codebases.** The retrofit story — one line to formally verified — is the entire value proposition. It only matters because nobody is going to rewrite their Express app in Dafny.

neurallog is reinventing refinement types, badly, with an LLM as the annotation engine. That's accurate and that's the point. The LLM writes worse annotations than a human expert. But it writes them for every codebase, automatically, from log statements that already exist. The perfect is the enemy of the deployed.

### What Refinement Types Do Better

- **Soundness.** A Dafny proof is sound. A neurallog proof depends on the LLM's contract being correct — and the LLM can be wrong. Z3 validates internal consistency, not semantic correctness.
- **Completeness.** Refinement types can express any property the type system supports. neurallog can only express what the LLM derives, bounded by the axiom set.
- **No convergence risk.** Refinement types don't have a convergence loop. The human writes the spec, the checker verifies it, done.
- **No LLM dependency.** The verification is deterministic and reproducible. neurallog's contract derivation depends on a non-deterministic model.

### What neurallog Does Better

- **Zero adoption cost.** One line. Existing code. Existing log statements. No new language, no new type system, no annotations.
- **Language agnostic.** Works on TypeScript, Python, Go, Java — whatever you already write. Refinement types are language-specific.
- **Automatic.** The programmer doesn't write specifications. The system derives them. This means formal verification scales to teams that don't have formal methods expertise.
- **Incremental.** The verification dial goes from 0 (stairs) to 3 (elevator). You start where you are and turn it up. Refinement types are all-or-nothing.
- **Runtime verification.** Refinement types are static. neurallog also checks live values in production.

### The Honest Pitch

If you can afford to write your system in F* or Dafny or Lean, do it. You'll get stronger guarantees than neurallog can provide.

If you can't — and almost nobody can — neurallog gives you formal verification from the log statements you already have. It's not as rigorous. It's not as complete. It's not as sound. But it exists for your codebase, today, with one line of code. And that's better than the nothing you had yesterday.

## Design Principles

- Every existing log statement is an intent signal, whether the programmer meant it that way or not
- The system hooks existing loggers — there is no neurallog API the programmer interacts with
- `import neurallog` is the entire adoption surface
- Contracts are derived from code context, not from log message strings
- Contracts define what context to capture, not the log call
- The system never modifies application source code
- Stack frame inspection provides all values — no instrumentation needed
- Runtime-first derivation: contracts are born the first time the code runs
- Cache invalidation by file content hash + principle hash: contracts stay current automatically
- Verification principles are append-only: the system's knowledge only grows
- The derivation prompt is assembled dynamically from the principles library
- New principles are self-validated before committing: tested against different code to confirm generalization
- Principles are portable across languages and codebases
- Discovery is agentic; derivation is deterministic; verification is formal
- The system self-hosts: its own log statements become contracts become proof entries
- An unsat condition is always actionable: either the contracts or the code needs a fix
- The principle set is monotonic (append-only); enforcement tier is feedback-driven (blocking/warning/advisory based on observed FP rates)
- The system degrades gracefully: even without Z3, it's still a logger
