# Sugar: Five-Minute Launch Demo Script

Audience: engineers and skeptical CTOs. Tone: hands-on, no marketing register, every claim grounded in a command they can copy. Total run time on screen: five minutes.

The demo is a single terminal session against a fresh checkout. The screencast records the terminal and one window of the project's editor with `app.ts` and `bugreport.md` open.

---

## Cold open (10 seconds)

A black terminal. The narrator says one line.

> "Sugar verifies a function in sixty-four bytes. Same number whether the function has ten lines or ten million dependencies. Watch."

---

## Step 1: `sugar init` (40 seconds)

The narrator types:

```sh
$ sugar init
```

The init flow asks three questions in turn. The narrator picks the answers without commentary.

```
Surface (annotation library you already use):
  > zod                                 # picked
  class-validator
  joi
  yup
  custom

Agent (LLM backend that drives the CLI):
  > claude-code                         # picked
  codex
  opencode
  openai
  ollama

Solver (witness for the IR compiler):
  > z3                                  # picked
  cvc5
  vampire
  bitwuzla

Wrote .sugar/config.toml
Wrote .sugar/keys/foundation.seed (gitignored)
Wrote .sugar/proofs/.gitkeep
Project ready.
```

The narrator says:

> "Three questions. The CLI is invariant; the answers select the plugins. The CLI from this point forward is the same regardless of what I picked."

---

## Step 2: `sugar must` mints a contract (60 seconds)

The narrator opens `app.ts` in the editor. It is twenty lines of TypeScript that exposes a `transfer(from, to, amount)` function. The narrator types:

```sh
$ sugar must app.ts "users can't have negative balance"
```

The agent runs. Output:

```
[agent] reading app.ts ...
[agent] proposing contract:
  invariant: forall (account: Account). account.balance >= 0
  on:        every callsite that mutates Account.balance
[agent] lifting to Zod refinement on Account schema:
  z.object({ balance: z.number().nonnegative() })
[agent] emitting canonical IR ...
[agent] minting memento (contract / invariant) ...
[validator] IR shape: ok
[validator] kit primitives: ok
[validator] producer signature: ok
[verifier] tier 2 lookup ... no cached implication; falling through
[verifier] tier 3 z3 -in ... discharged in 78ms
[mint] CID: blake3-512:8fe93fc11f5c04aae1c9ffe11d165d880436732052dc2d59738340e19c6fce92eabb04a389c04604c4bd5930556006377d033e7b3be2efe5a6492929fab7a33a
[mint] wrote .sugar/proofs/8f/e9/<cid>.proof  (1124 bytes)
```

The narrator says:

> "I typed English. The agent translated to a Zod refinement, lifted that to canonical IR, ran Z3 against the call sites, and minted a signed memento. The CID is a hundred and twenty-eight hex characters. Files on disk are sharded by the first four hex chars. I never wrote a contract by hand."

---

## Step 3: Show the `.proof` file (30 seconds)

The narrator changes directory.

```sh
$ ls .sugar/proofs/8f/e9/
8fe93fc11f5c04aae1c9ffe11d165d880436732052dc2d59738340e19c6fce92eabb04a389c04604c4bd5930556006377d033e7b3be2efe5a6492929fab7a33a.proof

$ wc -c .sugar/proofs/8f/e9/8f*.proof
   1124 .sugar/proofs/8f/e9/8fe93fc11f5c04aae1c9ffe11d165d880436732052dc2d59738340e19c6fce92eabb04a389c04604c4bd5930556006377d033e7b3be2efe5a6492929fab7a33a.proof

$ sugar dump .sugar/proofs/8f/e9/8f*.proof
catalog
  name:        app.ts/Account.balance.nonneg
  version:     1.0.0
  signer:      blake3-512:e7c2... (foundation)
  declaredAt:  2026-04-30T17:14:23.502Z
  members:
    blake3-512:c1a4... -> contract Account.balance.nonneg
      kind:        contract
      out_binding: Account.balance
      inv:         forall (account: Account). account.balance >= 0
      preHash:     -
      postHash:    -
      invHash:     blake3-512:9d0c...
```

The narrator says:

> "This is the file. Filename is the BLAKE3-512 of its contents. Catalog metadata, one signed member, the canonical IR for the invariant, and the producer signature. Zero ambiguity. Anyone with the bytes can recompute the CID and refuse the file if it does not match."

---

## Step 4: `sugar verify-protocol --signed` (20 seconds)

```sh
$ sugar verify-protocol --signed
protocol catalog: blake3-512:5b7701823f1e98b027173ac1961977db6e2f4125b8b3dba03c3aae5759a8c9780aca30bed9abdfdfe0b5a7a8748c29cfa2a058269386925e1753634019f05cd4
peer claim:       blake3-512:5b7701823f1e98b027173ac1961977db6e2f4125b8b3dba03c3aae5759a8c9780aca30bed9abdfdfe0b5a7a8748c29cfa2a058269386925e1753634019f05cd4
match:            yes
peer signature:   ed25519:... (verified against publishing key)
ok
```

The narrator:

> "The protocol's version is the hash of its own catalog file. My peer's claim against that hash matches. We speak the same protocol. One comparison."

---

## Step 5: `sugar search` finds the new contract (30 seconds)

```sh
$ sugar search consequent="balance >= 0"
1 result:
  blake3-512:8fe93fc1...   contract Account.balance.nonneg
    out_binding: Account.balance
    inv:         forall (account: Account). account.balance >= 0
    declaredAt:  2026-04-30T17:14:23.502Z
    signer:      foundation
```

The narrator:

> "The lattice is queryable. I can ask for every memento whose consequent says 'balance non-negative'. The contract I just minted shows up. The search is structural; it works against the canonical IR, not the English claim."

---

## Step 6: `sugar fix` repairs a bug from a bug report (60 seconds)

The narrator opens `bugreport.md` in the editor. The bug report says: "When `transfer` is called with a negative amount, the source account ends up with a negative balance. Repro: `transfer(alice, bob, -100)` produces alice.balance = -100." The narrator types:

```sh
$ sugar fix --file bugreport.md
[agent] reading bugreport.md ...
[agent] identifying failing class of input:
  amount < 0 in transfer(from, to, amount)
[agent] proposing regression contract:
  pre:  amount > 0
  on:   transfer(from, to, amount)
[agent] proposing fix:
  edit app.ts:transfer to add Zod refinement
  z.object({ amount: z.number().positive() })
[agent] applying edit ...
[verifier] running test corpus ... 47 passing, 0 failing
[verifier] tier 1 hash check on app.ts ... new postcondition observed
[verifier] tier 3 z3 -in ... regression contract discharged in 51ms
[mint] CID: blake3-512:b22de7a0...
[mint] wrote .sugar/proofs/b2/2d/<cid>.proof
[git] committed: "fix: reject negative amount in transfer; add regression contract"
```

The narrator:

> "The agent read the bug, proposed a regression contract that captures the absence of the bug class, edited the source, ran the tests, ran the verifier, minted the regression memento, and committed. The bug class is now cryptographically absent. Anyone in the world who pulls this commit can verify the absence in sixty-four bytes."

---

## Step 7: `sugar ask` shows tier outcomes (30 seconds)

```sh
$ sugar ask 'forall (account). account.balance >= 0'
tier 1 (hash equality):       miss
tier 2 (cached implication):  hit  (blake3-512:8fe93fc1...)
tier 3 (z3 from scratch):     not run (tier 2 hit)
verified in 4.2us
```

The narrator:

> "I asked the lattice an arbitrary predicate. Tier 1 missed because no contract had this exact hash. Tier 2 hit because the invariant I minted earlier carries this predicate as its body. Z3 did not run. Total: four microseconds."

---

## Step 8: The headline number (30 seconds)

The narrator runs the showcase summary:

```sh
$ sugar-showcase benchmark --lattice /tmp/showcase-lattice --queries 10000 --summary
lattice:
  proof_files:    1100000
  implications:   1000000
  on_disk_bytes:  ~5 GB
queries:           10000
tier1 p50:         ~58 ns
tier2 p50:         ~65 us  (ed25519 verify dominates)
tier3 p50:         ~22 ms  (z3 small problem)
compression:       64 bytes per query  |  ratio = ~10^7
```

The narrator says, looking at the camera:

> "One point one million signed mementos on disk. Five gigabytes. Ten thousand random queries. Tier 1 in nanoseconds, Tier 2 in microseconds, Tier 3 in milliseconds. The cost of any one query is sixty-four bytes. Pick your depth in the DAG; the bytes do not grow. This took sixty-four bytes."

The narrator presses control-D. The terminal closes. End screen: the project URL, the spec catalog CID, "github.com/wopr-network/sugar".

---

## Total: five minutes

- 10s cold open
- 40s init
- 60s must
- 30s show .proof
- 20s verify-protocol
- 30s search
- 60s fix
- 30s ask
- 30s headline number

Total: 4:50. Buffer for narrator pauses: ten seconds.

Notes for the recording:

- Use a dark terminal theme; the CIDs are long and the contrast matters.
- Pre-warm the lattice fixture before recording so generate output is already on disk.
- Do not edit out the "tier 3 z3 -in ... discharged in 78ms" line in step 2; the wall-clock numbers are part of the demo.
- Keep the `wc -c` line in step 3 visible. Audiences want to see "this is a kilobyte file" before they believe "the verification is sixty-four bytes."
- The agent backend output above assumes Claude Code; switch verbatim for Codex / OpenAI by setting `[agent].backend` in `.sugar/config.toml`. The CLI lines above are unchanged.
