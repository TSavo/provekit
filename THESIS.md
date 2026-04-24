# The Thesis

## Programmer intent is everywhere in code. Most of it is informal.

Every `printf`, every `console.log`, every `logger.info` — going back to the earliest programs that ever printed a value to check if it was right — is an implicit claim about what should be true at that moment. The programmer wrote it because they had a belief about the code. They just lacked the tools to express that belief formally, so they expressed it informally. They logged it, and they trusted their eyeballs.

The same is true of variable names like `safeBalance` and `validatedInput`, function names like `sanitizeHtml` and `ensureAuthenticated`, type annotations, assertion-style error messages, and TODO comments. The code contains a distributed, informal specification of how it's supposed to behave. Formalising that specification — or, more honestly, *attempting* to formalise it, with calibrated confidence about how faithful the formalisation is — is what provekit does.

## The fundamental problem of formal verification was "how do we get the specifications?"

For fifty years the answer was "convince developers to write them." That never worked at scale. Specifications are expensive, tedious, separate from the code. They drift. They get abandoned.

The LLM-plus-SMT genre — Lemur, SpecGen, Clover, and provekit among them — proposes a different answer. An LLM reads the code, extracts the implicit specification, translates it into SMT-LIB, and a solver checks it. The cost of writing a spec goes from "expensive developer time" to "one LLM call per signal."

This solves one problem and introduces another.

## The new problem: the LLM can be wrong, and when it is, the solver proves nothing useful.

An LLM translating TypeScript to SMT can miss — and routinely does miss — things like:

- JavaScript numbers are IEEE 754, not integers or reals. `Math.MAX_VALUE + 1 === Math.MAX_VALUE`. `0.1 + 0.2 !== 0.3`. Two "mathematically distinct" values can have identical bit patterns.
- JavaScript division by zero returns `Infinity`, `-Infinity`, or `NaN`. It does not throw. Z3 might prove "division by zero prevented" and the runtime will hand your code a `NaN` anyway.
- `||` replaces any falsy value with the default. `??` replaces only `null`/`undefined`. An encoder picking the wrong one produces a semantically different proof.
- `typeof null === "object"`. `NaN !== NaN`. Sparse arrays skip indices in `map` but not in `for` loops.

Every item above is a category where Z3 will happily prove things about the LLM's idealised abstraction while the actual code does something different. The central weakness of LLM-plus-SMT tools is that they have no layer that notices when the abstraction and the runtime disagree. "Proven by Z3" sounds authoritative; it means the encoding is internally consistent, which is a much weaker claim than "the code is correct."

This is where most tools in the genre stop. provekit doesn't stop here.

## The answer isn't to abandon the approach. The answer is to check the encoding.

provekit adds two oracles beyond the solver:

**Runtime harness.** For each claim Z3 proves, a second LLM writes a JavaScript test harness. The harness constructs concrete input from Z3's witness, loads the real function, executes it, and observes. If the observation contradicts Z3's verdict, the encoding was lossy. The LLM judge then audits the harness itself (did the harness actually test the claim, or did it rig the test?) before the cross-reference counts as evidence.

**Existing test suite.** When the project has tests, provekit invokes the ones that reference the target function and compares their outcomes to the harness verdict. If Z3 said unsat, the harness ran clean, and the user's own pre-existing tests also pass, the claim has agreement from three independent sources.

Three oracles. Agreement at the intersection is high-confidence. Disagreement is a finding — sometimes about the code, sometimes about the encoding, and provekit surfaces both.

## The principle library grows from real bugs, under adversarial validation.

When a signal pattern recurs enough times and the existing principle catalogue doesn't cover it, the tool generates a new principle — an AST pattern plus SMT template — via LLM synthesis. Before it enters the library, a different model runs an adversarial pass trying to construct false-positive and false-negative examples. Only principles that survive are added.

This is not the algorithmic induction of a mathematical theory. It's a validated, LLM-produced extension of a pattern library, gated by adversarial testing. The library grows, and the mechanical-template coverage increases over time, which reduces the per-contract LLM cost. The growth is real; the framing as "discovering mathematical truths" would be hubris.

## What you actually get, honestly.

You don't get "mathematical certainty about your code." You get:

- A calibrated confidence level per claim (three oracles, their agreements, their disagreements)
- A re-runnable SMT block for every verdict Z3 produced (`echo '...' | z3 -in` is real — it verifies Z3's math, not the encoder's faithfulness)
- A runtime harness for every proven property where the function is executable
- A list of encoding gaps: places where Z3 was confident but runtime refuted
- A principle library that accumulates validated patterns from your code's real bugs

You don't get:

- Proof that your code is correct. You get proof that your code, *as the LLM translated it*, is consistent with a property. The harness tries to close that gap empirically and often does; sometimes it can't.
- Regulator-accepted certification. If a compliance framework requires formal verification, it almost certainly requires a tool whose soundness is itself certified — Coq, Isabelle, Dafny, TLA+, maybe F*. provekit's three-oracle architecture is better evidence than raw test coverage but not a replacement for those.
- A world where software becomes mathematical. Software stays empirical. We add a verification layer that catches a class of bugs current tools miss. That's enough.

## Proof as one unit of trust among several.

Bug bounty platforms have been overwhelmed by LLM-generated reports that sound plausible but don't hold up. "Require a Z3 proof" is one response, and it's a good one — but it's only a complete response if triage also runs the harness and checks the encoding. A proof-required bounty program that accepts every `unsat` verdict without running the harness layer would trade plausibility-based slop for proof-shaped slop.

provekit's architecture is a concrete proposal for what that second layer looks like in practice. The submission form accepts SMT-LIB; the triage also runs the harness and cross-references tests; the signal is the intersection of three oracles' agreement, not just one's verdict.

## Beyond log statements: signals everywhere.

Log statements are a rich entry point — every codebase has thousands and they require zero code changes. They're not the only signal the tool reads. AST patterns (branches, loops, dangerous calls, arithmetic on parameters), type annotations, function names, comments, error messages, existing test assertions — all are informal expressions of programmer intent, and all feed the same five-phase pipeline.

Phase 1 finds signals. Phases 2–5 don't care where they came from. Adding a new signal generator is one file implementing a tree-sitter AST walk.

The log statement was a wedge, not the thesis. The thesis is this: **programmer intent is expressed informally throughout the code, and a carefully calibrated LLM-Z3-runtime loop can turn some of it into checkable claims about real behaviour.**

## The unfashionable commitment.

Most tools in this space optimise for the compelling demo: "watch us prove your code correct in thirty seconds." The compelling demos skip the soundness question because showing the gap feels like admitting the tool is weaker than it is.

provekit optimises for the honest demo: here's the SMT, here's the harness, here's whether your tests agree. Sometimes the answer is "Z3 proved a property but the harness says runtime disagrees and your tests back up the harness — the encoder got it wrong, here's the specific mismatch."

A tool that tells you when its own verdict is wrong is more trustworthy than a tool that doesn't — even when the first one reports fewer green checkmarks.

That's the thesis.
