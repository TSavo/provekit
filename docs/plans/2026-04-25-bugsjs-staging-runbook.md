# BugsJS Hand-Staging Runbook

**Operational record for the first hand-staged BugsJS bugs.** This document captures every step I take by hand so that, after enough calibration runs, we can identify which steps are mechanical (automate directly), which need judgment (delegate to an LLM with a precise prompt), and which require a human at the gate.

The principle: **automate from observed behavior, not from speculation about behavior.** The runbook is the data.

---

## Per-bug step list

### Step 0: Project staging (one-time per project)

For each BugsJS project (Express, Mocha, ESLint, Karma, Bower, Hexo, Hessian.js, node_redis, Pencilblue, Shields):

```bash
cd ~/bugsjs && git clone --depth=1 --no-single-branch https://github.com/BugsJS/<project>.git
cd ~/bugsjs/<project> && git fetch --tags --depth=1
```

The shallow `--no-single-branch` matters — we need every `Bug-*-{original,fix,test,full}` tag reachable, but we don't need their full history.

**What's mechanical:** the clone command, the tag fetch.
**What's judgment:** which projects to bootstrap from first. Probably Express because it's the codebase I have the most intuition about, smallest surface area, and broadest variety of bug shapes (middleware, routing, error handling, async — covers many capabilities).

### Step 1: Bug enumeration (one-time per project)

```bash
cd ~/bugsjs/express
git tag -l 'Bug-*-original' | sort -V | head -10
```

For Express, I expect to see Bug-1 through Bug-N where N is project-specific. Pick the lowest-numbered bug first. There's no semantic ordering signal in the IDs — Bug-1 is just whichever bug got cataloged first. I'll work in numerical order until I have enough samples to know what variety looks like.

**Mechanical:** the listing.
**Judgment:** none yet — just walk in order.

### Step 2: Bug context collection (per-bug)

For Bug-N in project P:

```bash
cd ~/bugsjs/<P>
git diff Bug-N-original Bug-N-fix --stat               # files touched, sizes
git diff Bug-N-original Bug-N-fix                      # full unified diff
git log Bug-N-fix -1 --format='%s%n%n%b'               # commit subject + body
git diff Bug-N-original Bug-N-test -- 'test/**'        # what was added to tests
```

**What I read by hand:**
- The commit message — what does the developer say they fixed?
- The diff — what code actually changed? Is it a single-concern fix or noisy?
- The test diff — does the test target the specific scenario, or is it a broad behavioral test?

**Decision gates:**
- Diff touches > 2 files → consider rejecting (noise). Skip if so.
- Diff is purely under `test/` or `__tests__/` → no production-code principle to harvest. Skip.
- Diff is mostly whitespace / lint / import reorder with one or two real change lines → rejection candidate. Could harvest the real-change lines manually but that's a calibration question.

**Mechanical:** running the git commands.
**Judgment:** the read-and-evaluate. This is the step I most want to record verbatim across the first several bugs — what does my "this is a clean fix" intuition look like? Some of it is automatable as filters (LOC count, file count). Some of it isn't (is the comment-only change still a bug fix?).

### Step 3: Synthesize a bug report

The fix loop's intake stage takes a prose bug report. The commit message IS that report, in shape. I rewrite it lightly:

```markdown
# Bug report (synthesized from express/Bug-N)

[copied subject line from commit]

[copied body from commit, minus issue references and contributor handles]

Located at: src/<file>:<approx-line>  (derived from diff hunk header)
Function: <name>  (derived from `git blame` or grep at the hunk's nearest function)
```

**What I do by hand:**
- Read the commit message
- Translate "fixed X" / "addressed Y" framing into "X is happening" / "Y is wrong" framing (intake parses bug *reports*, not retrospectives)
- Extract a file:line reference from the first non-test diff hunk
- Find the enclosing function name

**Mechanical:** the file:line extraction (regex on diff hunk header `@@ -X,N +Y,M @@`).
**LLM-helpful:** the framing rewrite. A small LLM prompt: "rewrite this commit message as a forward-tense bug report; preserve technical content; remove contributor names and PR/issue references." Cheap haiku call.
**Judgment:** is the bug report semantically faithful to the actual fix? I check by reading both.

### Step 4: Stage the buggy snapshot for analysis

```bash
git worktree add ~/bugsjs-staging/<P>-bug-<N> Bug-N-original
cd ~/bugsjs-staging/<P>-bug-<N>
provekit init                 # creates .provekit/
provekit analyze              # builds SAST DB
```

**Why a worktree:** so I don't pollute the main clone's checkout state, and so each bug has an isolated `.provekit/` to inspect after.

**What I check:**
- Did `provekit analyze` complete without errors? Some legacy code may have parser failures — log them but proceed.
- How many nodes did it index? Should be reasonable for the project size.
- Does my expected locus from Step 3 actually exist in the SAST? Quick check:

```bash
sqlite3 .provekit/provekit.db "SELECT id, kind FROM nodes WHERE source_start LIKE '%<approx-byte-offset>%' LIMIT 5"
```

**Mechanical:** all of step 4 except the sanity check.
**Judgment:** the sanity check — is the SAST seeing what I expect? If not, there's a substrate gap (capability missing, file not parsed, etc.) that BugsJS just surfaced.

### Step 5: Run B3 against the staged snapshot

This is where I find out: does the existing principle library cover this bug already?

```bash
# Pseudocode for what I'll actually do — once #98 lands, this is a real command
provekit recognize <bug-report-file>
```

**Possible outcomes:**

**A. RECOGNIZED.** B3 returned a principle from the library. I read the principle, re-read the diff, and ask:
- Is the principle's match query firing on the SAME node my expected locus pointed at? If yes, recognition is correct. If no, false-positive at the locus.
- Would the principle's stored `fixTemplate` produce a diff structurally similar to the production diff? If yes, the recognition is doing real work. If no, the principle matched but its fix isn't appropriate for THIS variant — that's a finding (the principle's `alternateShapes` need extension).
- Append BugsJS provenance to the principle. Move on to next bug.

**B. NOT RECOGNIZED.** B3 returned no match. I run the full novel-bug path with `imported: true` and the diff/test pre-populated. Step 6.

**Mechanical:** running B3.
**Judgment:** the "is this recognition correct?" check. This is high-stakes. Wrong recognition pollutes the library forever. I want to inspect every recognized hit on the first N bugs even after we automate.

### Step 6: Imported-mode loop run (only when not recognized)

```bash
# Pseudocode, once #98 + harvest infrastructure lands
provekit harvest --imported \
  --bug-report bug-N-report.md \
  --diff bug-N.patch \
  --test bug-N-test.ts \
  --provenance bugsjs-express-bug-N \
  --staging-only
```

`--staging-only` means: don't add to the live library, write to `.provekit/harvest/staging/<bugClassId>/<source-id>/` for me to inspect.

**What runs:**
- Intake (LLM, haiku): parses my synthesized bug report into a BugSignal.
- Locate (mechanical): finds the locus in the SAST.
- Classify (LLM, haiku): routes the bug to layers.
- C1 (LLM, opus, mode=llm): generates an invariant from the bug report. Oracle #1 verifies it's SAT.
- C1.5 (multiple LLM calls): invariant fidelity. May fail; retry up to 1; failures land in the log for me to read.
- C2 (mechanical): opens the overlay (a fresh worktree branched from the buggy snapshot).
- **C3 (imported)**: applies the bug-N.patch to the overlay. NO LLM. Oracle #2 verifies the patched invariant is unsat.
- C4 (LLM, sonnet): looks for complementary sites where the same bug pattern might hide. May find none.
- **C5 (imported)**: writes bug-N-test.ts directly. Runs it. Oracle #9 mutates the fix and confirms the test fails on the mutation.
- C6 (LLM, opus): abstracts the (signal, invariant, diff) triple into a DSL principle.
- D1 (mechanical): assembles the bundle, runs all bundle-coherence oracles.
- D2: in `--staging-only` mode, NOT applied to the project — written to staging dir.

**What I inspect by hand on each first-staging:**
- The C1 invariant. Reads sensibly?
- The C1.5 outcome. Did fidelity pass cleanly or did it retry? If retried, what was the disagreement?
- C4's complementary findings. Real or noise?
- The C6 principle. **This is the harvest output.** Does the DSL read like English? Does the SAST query match at the expected node and only there? Does the `fixTemplate` look like it would generalize to a syntactic variant of this bug?
- All bundle oracles green?

**Mechanical:** the full loop run, the staging directory write.
**Judgment:** the inspection. Especially C6's principle. Almost-everything-else has a verdict; C6's output requires taste.

### Step 7: Promote-or-quarantine decision

**For each staged principle**, I decide manually:

- **Promote**: principle is high quality. Move to `.provekit/principles/<bugClassId>.dsl`.
- **Quarantine**: principle has a defect (too narrow, too broad, ambiguous). Leave in staging with a notes file recording what's wrong. Revisit during calibration review.
- **Discard**: principle is fundamentally broken (false-positives across the corpus, or doesn't catch the bug it was harvested from). Delete.

**Cross-bug dedup decisions:**
- If a new staged principle has the same `bugClassId` as an existing library principle, examine: is it a syntactic variant (extend `alternateShapes`) or a semantically distinct sub-class (separate principle)?
- If two staged principles in the same batch have the same `bugClassId` but different shapes: merge them into one principle with `alternateShapes`, OR keep them separate if the shapes are independently meaningful.

**Mechanical:** none. This is entirely judgment.
**LLM-helpful:** "given these two principles, are they the same bug class?" — but the decision needs human sign-off until we've calibrated.

### Step 8: Notes for the calibration record

After each bug, I append to `docs/plans/2026-04-25-bugsjs-calibration-log.md`:

- Bug ID + project
- Outcome (recognized / harvested / quarantined / discarded)
- Wall time
- LLM cost (sum of stage costs)
- Surprises (anything that didn't match my expectation)
- Decisions I made by judgment that should eventually be automated

After 10-30 bugs, the log becomes the spec for which steps to automate and which to keep human-in-the-loop.

---

## Default to the LLM for semi-structured text operations

**Rule:** when the input is human-written text or code-like content where intent matters more than format, reach for the LLM. Don't write brittle regex to parse what the LLM can read in one shot.

Specific places this applies during staging:

1. **Function name from diff hunk.** Instead of `git blame` + grep gymnastics, paste the diff and ask "which function does this change belong to?". The LLM reads the surrounding context with no parsing fragility.

2. **Bug report synthesis (Step 3).** Translating commit-message retrospective tense into bug-report forward tense. Small haiku call.

3. **Test fragment reconstruction.** If a BugsJS test imports something the overlay doesn't have, or uses a test helper we don't recognize, ask the LLM to rewrite the test so it stands alone with our infrastructure. Don't parse the test's AST and try to substitute symbols by hand.

4. **Diff cleanliness assessment (Step 2).** "Is this commit a clean bug fix, a fix-plus-refactor, or noise?" — judgment question. Cheap haiku call returns a verdict + one-sentence rationale. Decision still mine, but the LLM does the read-and-summarize.

5. **Provenance dedup (Step 7).** "Are these two staged principles the same bug class?" — sonnet call with both DSLs + sample matches. Faster than parsing structurally identical ASTs by hand.

6. **Calibration log summarization.** After 30 bugs, ask an LLM "what patterns do you see?" Surfaces things I missed.

These are *staging-operation* LLM calls — distinct from the loop's intrinsic LLM calls (intake, classify, C1, C4, C6). Cheap, single-shot, often haiku.

The principle: **regex for deterministic formats, LLM for human-written text.** A diff hunk header `@@ -X,N +Y,M @@` is deterministic — regex it. A commit message describing the fix is human prose — LLM it.

## What I will NOT delegate to an LLM during hand-staging

- The promote/quarantine/discard decision (Step 7)
- The "is this recognition correct?" check (Step 5A)
- The "is this commit message describing a real bug fix or just a refactor labeled as a fix?" check (Step 2)
- The decision to skip a noisy diff (Step 2)
- Reading and signing off on each harvested principle's DSL

These define the calibration data. If I delegate them, I have no record of what good vs bad looks like, and the eventual automation will be an unguided LLM, not a learned process.

---

## Operational ordering

When #98 lands and we begin:

1. Project setup: Express only (defer other projects until we've calibrated on Express).
2. Walk Bug-1 through Step 8. Together.
3. Iterate: I propose changes to this runbook based on what I observed. You sign off.
4. Bug-2 through Bug-5 with the calibrated runbook.
5. After Bug-5: revisit. Are we ready for batch-of-10 with my supervision but no per-bug sign-off? Or are there still gates that need attention?
6. Continue widening the trust boundary as the runbook stabilizes.

The endpoint isn't full automation; it's the *minimum-supervision* automation we can defend. The runbook tells us where.
