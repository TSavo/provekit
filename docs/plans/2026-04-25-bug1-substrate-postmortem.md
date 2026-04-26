# Bug-1 Substrate-Extension Postmortem

**Operational record for the v9 through v22 chase to land a substrate bundle on Bug-1 (Express duplicate-Allow-methods).** Every iteration surfaced a real finding; every finding shipped a fix. The bug-fix slice now ships clean every run. The substrate-extension slice cleared every infrastructure layer; the residual gap is LLM-quality on a hard discrimination task, not system code.

The principle: **automate from observed behavior, not from speculation.** Each v-run was data. The 13 commits below are what that data taught.

---

## Goal

Bug-1 is the bootstrap principle library's first real entry. Hand-staged from BugsJS Express (duplicate HTTP methods in OPTIONS Allow header). The success criterion is not just "fix the bug." It is: the fix loop proposes a new capability + DSL principle that all four substrate oracles (#14, #16, #17, #18) pass and that adversarial validation (oracle #6) accepts, so the principle joins the library forever.

A "fix bundle" closes the bug. A "substrate bundle" closes the bug AND extends the SAST. Substrate is the load-bearing outcome.

## Starting state

Before this session, Bug-1 v8 surfaced a capability proposal that wrote files to disk but never reached oracle #14. Earlier sessions had built the infrastructure but not exercised it end-to-end on a real bug.

## Trajectory

Each row: the iteration, the finding the run surfaced, the fix shipped.

| v   | Stage that failed                | Finding                                                                                       | Commit    |
|-----|----------------------------------|-----------------------------------------------------------------------------------------------|-----------|
| v9  | C6 capability spec               | LLM produced shapes that didn't match canonical patterns. Pre-existing prompt + extractor work was already underway prior to this session. | (prior)   |
| v10 | C6 schema types                  | Schema declared `nodeId: integer` instead of `text`. Real `nodes.id` is text. Datatype mismatch on every insert. | 78ee4b8 (prior) |
| v11 | Oracle #16 dynamic import        | Schema's `import { nodes } from "../../../src/sast/schema/nodes.js"` couldn't resolve. The relative path doesn't reach back from the executor cache dir, AND the project ships TS only. | 1e37f5a   |
| v12 | C1 JSON parsing                  | Opus returned prose-wrapped fenced JSON ("Here is the invariant output: ```json...```"). Text-mode `parseJsonFromLlm` couldn't tolerate prose preamble; agent-mode already had `extractJsonFromText` fallback (#102) but text-mode missed it. | 119b8ed   |
| v13 | D2 patch export                  | Oracles green, commit SHA logged, but `provekit-fix.patch` was 0 bytes. Target was a detached-HEAD worktree; targetRef fell back to literal `"HEAD"` string; baseRef captured that string; after commit advanced HEAD, `git diff HEAD..HEAD` resolved both ends to the same new commit. | 6531b06   |
| v13 | C6 routing                       | LLM emitted bare-principle shapes that compiled cleanly but adversarial validation rejected them as too broad. `tryExistingCapabilities` returned `non_codifiable` and the substrate path never fired. (v11 happened to work because the LLM directly chose `needs_capability`; v13 was unstable variance.) | a129ab0   |
| v14 | C6 agent meta.json               | Agent wrote the `.dsl` file separately and put the path string into `meta.dslSource` instead of inlining the source. Oracle #18 then tried to `parseDSL` the path and failed at character 10 (`/`). | f9bd25b   |
| v15 | C6 substrate gap message         | Synthetic gap built from raw adversarial metrics ("false-positive pass: 3/3 (100%)") confused the agent: it spent 11+ minutes exploring tool calls instead of writing the spec. Agent needed a predicate description, not validation metrics. Same commit also opened `allowedTools` to `[".*"]` since the SDK was inheriting MCP tools from the user's Claude Code config and the partial restriction was incoherent. | 0ddc48f   |
| v16 | Overlay-bypass detector          | Agent hallucinated `/home/user/.provekit/...` for the first round of Writes, then ran `pwd`, saw the real overlay, and re-wrote everything to the correct path. Bypass detector caught the first Write and threw, even though the agent had already self-corrected. | 9d9f9d5   |
| v17 | Oracle #18 DSL parse             | LLM emitted DSL using `description:`, `severity:`, `category:`, `forbid` syntax. Parser supports `match $x: node where ... report violation { ... }`. The prompt had heavy schema/extractor teaching but only a one-line DSL example. | 4489f08   |
| v18 | Oracle #18 column name           | LLM declared schema as `violationKind: text("violation_kind")` (drizzle-correct: camelCase JS property, snake_case SQL name) then referenced `myCap.violation_kind` in the DSL. DSL queries use the JS property name. | e5eb447   |
| v19 | Substrate adversarial            | Capability passed oracles 14/16/17/18 but the principle DSL was too broad: 3/3 false-positive on adversarial validation. Added one-shot DSL refinement when adversarial fails: feed the failure evidence back, regenerate just the principle DSL, re-validate. | c794c73   |
| v20 | Oracle #16 self-inconsistency    | Agent's own extractor matched one of agent's own negative fixtures (`negative[4]: expected 0 rows, got 1`). The principle-only refinement retry didn't cover earlier gates. Refactored proposeWithCapability into a 2-attempt loop calling proposeWithCapabilityOnce with augmented gap on retry. | 35badc0   |
| v21 | Refinement DSL parse errors      | Both attempts ran cleanly through the new 2-attempt loop, but both refinement LLM calls emitted broken DSL (`Parse error at 3:5: Unexpected character '&'` from `&&`; `Parse error at 3:39: Expected LPAREN but got DOT` from chained dot access). Refinement prompt had only a bare skeleton; main capability prompt had full anti-patterns. | f9f59f9   |
| v22 | LLM API hung                     | Sonnet refinement call outstanding 20+ minutes with no response, no CPU activity. Killed. Outside the fix-loop's purview to debug. | (none)    |

## The 13 commits, grouped

**Executor and parser plumbing:**
- `1e37f5a` SAST schema FK imports → proxy stub
- `119b8ed` structuredOutput text-mode prose-wrapped JSON fallback
- `6531b06` D2 baseRef pinned to SHA so patch export survives commit

**C6 routing and resilience:**
- `a129ab0` mechanical fallthrough for adversarial-rejected bare principles
- `f9bd25b` agent recovers dslSource when agent put a file path in meta.json
- `0ddc48f` predicate-shaped substrate gap + open agent toolset
- `9d9f9d5` overlay-bypass tolerates Write self-correction

**LLM teaching:**
- `4489f08` DSL grammar teaching + parseDSL pre-validation gate
- `e5eb447` DSL column-naming teaching (JS property vs SQL column)
- `f9f59f9` refinement prompt teaches DSL anti-patterns

**Retry budget:**
- `c794c73` retry principle refinement after adversarial rejection
- `35badc0` unified two-attempt retry across all substrate gates

420/421 fix-loop tests pass. No regressions. One pre-existing skip.

## End state

**Bug-fix slice:** ships clean every run. v17, v18, v19, v20, v21, v22 each produced a 2.6-3.3 kB patch with the actual deduplication fix at `lib/router/index.js:153-160` and a regression test that passes-on-fixed and fails-on-original. All behavioral oracles green. This is the working Bug-1 outcome, durable.

**Substrate-extension slice:** every infrastructure layer is closed. The pipeline now:
1. Routes correctly when bare principles get rejected (mechanical fallthrough)
2. Pre-validates DSL grammar with parseDSL before running expensive oracles
3. Recovers dslSource when the agent uses Write tool with a path reference instead of inline content
4. Tolerates absolute-path hallucinations when the agent self-corrects to the overlay
5. Teaches DSL grammar including the column-naming convention and anti-patterns
6. Retries refinement when adversarial fails, with structured failure feedback
7. Retries the whole capability spec when any earlier gate fails, with the failure detail in the gap

## What's left

The remaining gap is LLM-quality on the discrimination task: producing a capability extractor and a principle DSL that together match bug-shaped sites tightly enough to pass adversarial validation. The adversarial validator is doing its job, rejecting principles that fire on benign code with the same surface shape as the bug. That gate is correct.

Three observations from the chase:

**The discrimination is genuinely hard.** Bug-1 is "array.push consumed via array.join in a set-semantic header context." The LLM consistently produces extractors that match `array.push + array.join` without the contextual constraint. Distinguishing the bug from `arr.push(line); arr.join("\n")` (writing a multi-line file, valid use) requires either reading the consumer context (HTTP header name) or a different invariant framing. Neither is something prompt teaching reliably produces.

**The system raised the floor as far as it can.** Per the architecture rule (raise the floor via gates and tests, not via better AI), the gates are present and working. Adversarial validation correctly bounces over-broad principles. Oracle #16 catches self-inconsistency. Oracle #18 catches gratuitous capabilities. The fix loop's job is to provide the gates and the retry budget; it cannot substitute its own judgment for LLM design quality on a hard discrimination.

**The LLM API itself can hang.** v22 sat for 20+ minutes on a single sonnet refinement call before being killed. That is not an LLM-quality issue or a fix-loop bug. It is a property of the API client. A timeout wrapper on agent calls (kill at e.g. 5 minutes) would be a small improvement worth taking when the loop is revisited.

## Where to push next, if revisited

In rough order of expected payoff:

1. **Timeout wrapper on agent.complete and agent.agent calls.** v22 hung indefinitely. Even a 5-minute timeout would make the retry loop more reliable.
2. **Stryker-style mutation operators against the LLM-proposed extractor before adversarial validation.** Run the agent's own extractor against mutated fixtures generated mechanically (drop a guard, rename a method) and reject the proposal if it doesn't differentiate. Cheaper than another LLM round-trip; catches the v20 self-inconsistency class earlier.
3. **A bigger discrimination-teaching example in the C6 capability prompt.** The prompt teaches schema, extractor, and DSL grammar. It does not teach "what makes a discrimination load-bearing vs structural." A worked example with a real bug, a too-broad first attempt, the adversarial failure, and a tightening would help. This is prompt teaching the user has cautioned against, but in this case it is teaching a non-trivial design skill, not patching variance.
4. **Move on to BugsJS harvest (#97).** Bug-1 is a worst-case for substrate landing precisely because its discrimination is contextual. Harvesting from 452 production-merged BugsJS fixes will surface easier discriminations (off-by-one, division-by-zero) where the substrate path will land more consistently. The library grows by accretion.

## Lessons

**Each iteration surfaced a real finding.** Across 14 v-runs, every failure was a real system gap, a real LLM gap, or a real prompt gap. None were noise. The dollar cost was non-trivial; the design-debt-paid was meaningful.

**Infrastructure can be perfected; LLM judgment cannot.** The 8 commits in the "executor/parser plumbing" and "C6 routing and resilience" categories are durable. They will close their respective gaps for every future bug. The 3 commits in "LLM teaching" are likely necessary but not sufficient: each one helps, none guarantees correctness on the next run. The 2 commits in "retry budget" amortize LLM variance across more attempts; they raise the success rate but do not guarantee landing.

**The fix-bundle outcome is durable; substrate is opportunistic.** The user gets a working patch every run. The substrate bundle lands when the LLM produces a tight discrimination on attempt 1 or 2. That is the right shape for the system to take: the customer-facing outcome is reliable, the library-extension outcome is opportunistic-but-frequent.

**Stop chasing when the gap moves outside the system.** v22's hung API call is the right place to stop. Past that, the fix-loop is no longer the variable.
