# Agent worktree path resolution bleed: audit

**Status:** open (no fix landed)  
**Discovered:** 2026-04-25, during Bug-1 substrate-extension dogfood (v16)  
**Re-surfaced:** 2026-05-02, during PR #18 Go cross-impl conformance dispatch  
**Severity:** infra hazard for parallel agent dispatch (many agents in different worktrees)  
**Current mitigation:** post-agent bypass detection in `captureChange.ts` (commit `9d9f9d5`), C3/C6 fix-loop agents only; no protection for general Task/subagent dispatch

---

## 1. Architecture: how agents create worktrees

Claude Code's parallel agent dispatch creates git worktrees under `.claude/worktrees/agent-XXXXXXXXXX/`.
Each is a `git worktree add --detach` (or a branch checkout) producing a full working tree that shares the
main repo's object database. The `.git` file in each worktree points to `.git/worktrees/agent-XXXXXXXXXX/`.

Worktrees observed on disk (2026-05-02):

```
.claude/worktrees/
  agent-a0f20394773d03c5b/   # feat/go-cross-impl-... (PR #18)
  agent-a1e8500a548dfaa33/   # feat/go-cross-impl-... (PR #18 parallel)
  agent-a2c24ebaf00c56578/   # feat/py-cross-impl-... (Python port)
  agent-a40f6b0d3d33ab0f6/   # docs/manifesto-dag-is-a-tape
  csharp-cid-bump/           # fix/ci-csharp-cid-bump
  conformance-fix/           # fix/conformance-script-local-bug
  ... (44 total)
```

Agent configs live in `.claude/agents/` (e.g. `implementer-against-spec.md`) and define the agent's
tool set (Bash, Read, Write, Edit, Glob, Grep), model binding, and scope boundaries.

## 2. The bleed: root cause

When an agent task prompt contains an absolute path referencing the maintainer's main worktree
(e.g. `/Users/tsavo/provekit/src/workflow/producers/foo.ts`), the agent's Edit/Write tools resolve
that path literally on the filesystem. The agent is executing in an isolated worktree at
`/Users/tsavo/provekit/.claude/worktrees/agent-XXXXXXXXXX/`, but the absolute path points to the
maintainer's primary clone at `/Users/tsavo/provekit/`.

**Why this happens:**

1. Task dispatch prompts are authored from the maintainer's main worktree and include absolute paths
   (e.g. "Modify `src/workflow/producers/foo.ts`" becomes `file_path: "/Users/tsavo/provekit/src/workflow/producers/foo.ts"`).
2. The agent's worktree is a full clone -- the same directory hierarchy exists under a different root.
3. The LLM sees a string path, not a filesystem context. It passes it verbatim to the tool.
4. The tool implementation resolves the path against the real filesystem. `/Users/tsavo/provekit/`
   is the maintainer's main worktree, not the agent's.

**Consequence:** a Write tool call against `/Users/tsavo/provekit/src/foo.ts` mutates the maintainer's
working tree, not the agent's isolated copy. If multiple agents are active (e.g. one porting Go conformance
fields, another porting Python conformance fields, a third running a conformance harness), all three can
race on the same files.

## 3. Existing partial mitigation

Commit `9d9f9d5` (`fix(c6): tolerate Write-bypass when agent self-corrected to overlay path`) added
post-agent path enforcement in `src/fix/captureChange.ts`:

```
2. Layer 2: post-agent path enforcement.
   Inspect every tool use to detect file accesses outside the overlay.
   Edit/Write/Read are hard-fail: any absolute path outside the overlay root
   is a confirmed bypass.
```

The algorithm works in two passes:

1. **Collect:** scan every tool use for `file_path` parameters. If the path is absolute and resolves
   outside the overlay root, record it as a bypass event. Also record all in-overlay paths.
2. **Decide:** for each bypass event, if it is a Write and the same relative tail (e.g. `.provekit/foo`)
   was also written inside the overlay, tolerate it (self-correction). For Read/Edit bypass events,
   throw `OverlayBypassError`.

**Scope of protection:** this runs inside `runAgentInOverlay()` which is called by C3 and C6 stages
(fix-loop agent pipeline). General Task/subagent dispatch (like the agents used for PR #18) does NOT
go through this function.

**What the bypass detector catches and tolerates:**

- Agent hallucinates `/home/user/.provekit/foo`, runs `pwd`, sees real overlay path, re-writes everything
  under the correct path. The bypass detector tolerates the first Write because a later Write with the
  same `.provekit/`-rooted tail happened inside the overlay.
- Read/Edit bypasses always throw because those touch real existing files outside the overlay.

**What the bypass detector does not catch:**

- Writes that land on the maintainer's main worktree without a corresponding self-correction Write.
  If the agent only issues a single Write to `/Users/tsavo/provekit/src/foo.ts`, it's a silent mutation.
- Any tool use from a general Task/subagent dispatch (not going through `runAgentInOverlay`).
- Concurrent writes from multiple agents -- the detector is post-hoc (agent has already finished).

## 4. Failure modes observed in dogfood

| Failure                                                        | Observability  | Root cause                                                                   |
|----------------------------------------------------------------|----------------|------------------------------------------------------------------------------|
| Agent writes to `/Users/tsavo/provekit/src/` (main tree)       | Silent         | Prompt contains absolute paths; agent uses them literally                    |
| Agent writes to `/home/user/.provekit/` (hallucinated)         | Detected + tolerated | Agent hallucinates a Linux home path on macOS; self-corrects after `pwd`|
| Agent edits a real file outside overlay                        | Caught (throw) | `OverlayBypassError` thrown in `captureChange.ts`                            |
| Agent dispatches parallel task and both agents write to main tree concurrently | Silent | No coordination; no CWD enforcement on Task/subagent dispatch                |
| Agent stash-recover (`git stash` before commit) hides bleed    | Invisible      | Agent's own recovery masks the fact that it touched the main tree            |

The maintainer's `git stash` recovery flow (run stash, clean working tree, re-apply in worktree) is a
second-order symptom: it hides the bleed but does not prevent it. The agent "recovers" by discarding
stray writes, but if another agent committed between the write and the stash, that commit could include
the bleed content.

## 5. Impact for parallel agent dispatch

With 44 worktrees active, the blast radius is significant:

- **Concurrent writable target:** every agent's tool calls can reach the maintainer's main worktree
  via absolute paths. Two agents writing to the same file simultaneously produce a git merge conflict
  immediately.
- **Cross-agent contamination:** agent A writes to main tree; agent B reads from main tree; agent B's
  logic is now based on agent A's uncommitted mutations. Silent corruption that produces no merge
  conflict but may generate incorrect output.
- **Commit poisoning:** if an agent's bleed-write is followed by `git add` + `git commit` in the
  maintainer's tree, the bleed content is committed. The agent's own commit (in the worktree) is
  clean; the pollution is in the shared main tree's history.

## 6. Structural fix options

### (a) CWD invariant: validate every tool call's paths are under the worktree root

**Mechanism:** wrap Edit/Write/Read tool implementations so that before executing, they validate:
`resolved_path` must be a prefix descendant of `current_worktree_root`. If not, reject with a
descriptive error and log the full request.

**Pros:**
- Catches at enforcement point regardless of how the absolute path entered the agent's context.
- Prevents writes rather than detecting them post-hoc.
- No prompt rewriting required; no filesystem tricks.
- Fails loudly (agent sees the rejection and can self-correct).
- Already the architectural pattern used in `captureChange.ts`.

**Cons:**
- Requires instrumentation at the tool dispatch layer (Claude Code internals or pre-dispatch hook).
- May interrupt agent execution if the agent has no fallback path -- but the agent can usually
  recover by re-trying with a relative path.

### (b) Strip absolute paths from agent prompts

**Mechanism:** before dispatching a Task/subagent, scan the prompt for absolute paths pointing
to the maintainer's main worktree and rewrite them as relative paths (e.g. `/Users/tsavo/provekit/src/foo` -> `./src/foo`).

**Pros:**
- Prevents the issue at the source (prompt authorship).
- Simple implementation: a regex on the prompt string.

**Cons:**
- Fragile: the agent may still construct absolute paths in its own reasoning (e.g. from reading
  file headers, from `pwd` output, from tool results that include absolute paths).
- Does not protect against hallucinated absolute paths.
- The "strip" may be wrong if the dispatch prompt accidentally contains an absolute path that
  genuinely refers to a non-project location.

### (c) Per-agent symlink alias

**Mechanism:** create a symlink at `/Users/tsavo/provekit/.claude/worktrees/agent-XXX/Users/tsavo/provekit`
pointing to the worktree root, so that `/Users/tsavo/provekit/src/foo.ts` inside the worktree resolves
to the agent's copy.

**Pros:**
- No tool instrumentation required. Paths resolve naturally.

**Cons:**
- Breaks git's internal path tracking. Git worktrees do not expect symlink aliasing.
- Platform-specific (symlink permissions, macOS SIP restrictions, Linux container isolation).
- Non-portable across machines (the maintainer path `/Users/tsavo/provekit` is not universal).
- Confuses the LLM further (tools that show realpath output will reveal the worktree path, not the
  aliased path).

### (d) Wrap Edit/Write to resolve relative-to-worktree-root

**Mechanism:** when an Edit/Write tool receives an absolute path, resolve it relative to the
worktree root (strip the prefix `/Users/tsavo/provekit` and prepend the worktree path).

**Pros:**
- Silent correction -- agent never sees the error.
- Prevents write corruption transparently.

**Cons:**
- Masks the problem. The agent's mental model of "I'm writing to /Users/tsavo/provekit/src/foo.ts"
  is silently mapped to the worktree. If the agent later reads the file via an absolute path,
  it gets the main tree's version -- inconsistency that produces subtle bugs.
- Similar fragility to (b): what about paths that are genuinely meant to be absolute (e.g. system
  config files)?

## 7. Recommendation: (a) CWD invariant at the agent dispatch boundary

Option (a) is the correct structural fix. The reasoning:

1. **Enforcement at the enforcement point.** The tool layer is the only place where the agent's
   intent (a path string) meets the filesystem (a real inode). Any validation that happens later
   in the pipeline (post-hoc detection) or earlier (prompt rewriting) can be bypassed. The tool
   layer is the choke point where bypass is impossible.

2. **Fail-loud, not fail-silent.** Unlike option (d), which silently remaps paths and can produce
   inconsistencies, option (a) surfaces the rejection to the agent. The agent sees the error, can
   reason about it, and can self-correct by issuing a relative path. This matches the existing
   `captureChange.ts` design principle: "This is detection, not prevention. By the time we get here
   the agent has already run." The fix is to move that detection UPstream, from post-hoc to pre-tool,
   while keeping the fail-loud semantics.

3. **Generalizes across dispatch modes.** The fix-loop overlay path (`runAgentInOverlay`) and the
   general Task/subagent dispatch path both resolve tool calls. A validation hook at the tool dispatch
   layer protects every agent uniformly, regardless of how it was spawned.

4. **Already partially implemented.** `captureChange.ts` already demonstrates the pattern: resolve
   the worktree root, compare, reject. The delta is: move the check from after the agent runs to
   before each tool executes, and apply it to the general dispatch path.

**Implementation sketch:**

```
For every Edit/Write/Read tool call in the agent dispatch layer:
  1. Resolve the tool's file_path against the filesystem (realpath).
  2. Resolve the agent's worktree root (realpath of the cwd assigned during worktree creation).
  3. If file_path does NOT start with worktree_root:
     a. Log the full tool input for audit.
     b. Return a tool_result with an error message:
        "Path '/Users/tsavo/provekit/src/foo.ts' is outside the agent's
         worktree at '/Users/tsavo/provekit/.claude/worktrees/agent-XXX'.
         Use a relative path (./src/foo.ts) or the worktree-relative
         absolute path."
     c. Do NOT execute the tool.
  4. Otherwise, execute normally.
```

**Scope of instrumentation:**
- General Task/subagent dispatch: add the check to the tool call handler (Claude Code level or
  provekit's agent orchestration layer).
- `runAgentInOverlay`: keep the existing post-hoc detection as a second line of defense, but
  add the pre-tool check as the primary line.

**Risk of breakage:** low. The only case where an agent legitimately needs to write outside its
worktree is for build artifacts in system temp directories, which do not use Edit/Write tools
(they use Bash with redirects or install commands).

## 8. What not to do

- **Do not add more prompt teaching.** The Bug-1 postmortem (v16) already tried CWD-in-prompt.
  Teaching the LLM its absolute path does not prevent hallucination; it just changes the hallucination
  to a different absolute path.
- **Do not rely on git stash recovery as a safety net.** The recovery hides the problem; it does
  not prevent concurrent writes from two agents racing on the same file.
- **Do not wrap only the fix-loop path.** PR #18's agent was a Task/subagent dispatch, not a fix-loop
  C3/C6 agent. The protection must cover every agent dispatch mode.

## 9. References

- Commit `9d9f9d5`: `fix(c6): tolerate Write-bypass when agent self-corrected to overlay path`
  (`src/fix/captureChange.ts` -- post-hoc bypass detector for fix-loop agents only).
- `docs/LOGGING.md` line 14: initial recognition of the bypass bug as a logging visibility problem.
- `docs/plans/2026-04-25-bug1-substrate-postmortem.md` line 33 (v16): "Agent hallucinated
  `/home/user/.provekit/...` for the first round of Writes, then ran `pwd`, saw the real overlay,
  and re-wrote everything to the correct path."
- `docs/plans/2026-04-25-bug1-substrate-postmortem.md` line 29 (v13): overlay-bypass via detached-HEAD
  worktree (`git diff HEAD..HEAD` resolved to same commit).
- `docs/plans/2026-04-25-production-readiness.md` line 79: flagged stale worktree state and worktree
  cleanup races as production risks.
- PR #18 (`feat/go-cross-impl-binary-cid-target-proof-cid-source-contract-cid-evidence`): the Go
  cross-impl port agent that triggered this audit. Agent worked in `.claude/worktrees/agent-a1e8500a548dfaa33/`.
