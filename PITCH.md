# neurallog

Your log statements, type annotations, function names, and TODO comments are informal assertions about how your code is supposed to behave. neurallog turns them into checkable ones.

```
npm install -D neurallog
npx neurallog init
```

Here's what actually happens, end to end:

1. An LLM reads each signal point in context and writes an SMT-LIB encoding of the property that should hold at that point.
2. Z3 checks the encoding. `unsat` on the negated goal means the property is consistent with the LLM's abstraction of the code. `sat` means Z3 found a counterexample.
3. A second LLM — or a mechanical template match, when the pattern is known — generates a JavaScript harness and runs the real function with inputs derived from Z3's witness. If the harness contradicts Z3's verdict, we've found an **encoding gap**: the LLM's translation of your code was lossy in a way that matters.

What you get is **not** "mathematical certainty." The tool's central honesty is about that. Z3 is sound over the SMT encoding; the encoding was produced by an LLM reading your code; the LLM can be wrong, and when it is, the proof is about a fictional function, not yours. Most tools in this genre ignore that gap. We look for it on purpose, with a runtime harness and — when your project has one — your own test suite as a third oracle.

What you get is three independent signals — a Z3 proof, a runtime execution, and existing tests when present — agreeing or disagreeing per claim, with disagreements surfaced as findings. A claim all three agree on is high-confidence. A claim only two agree on is annotated. A claim Z3 signed off on but harness or tests refute is an encoding gap, filed as a bug.

The install is one command. The machinery is LLM-plus-Z3-plus-runtime. The output is a verification dial you control, not a compliance certificate we hand you.

neurallog.app
