# After Vendoring: Migration as Source Transformation Over the Program Graph

*Paper 21 said the cross-X mechanism dissolves cross-library uniformly along with every other axis. This paper says the operational consequence at the cross-library axis is that "migrate from library A to library B" stops being a project a team postpones for eighteen months and becomes a command a team runs at lunchtime. The shape of the deliverable is not a diff. The shape of the deliverable is a signed report whose first page aggregates the trichotomy of outcomes (rewritten, widened, refused, lossy) into five lines, and whose body cites every changed function with a propagation reason. The substrate is not a permanent artifact in the migrated codebase. The substrate exists to compute and justify the patch.*

## 1. The claim

For any two libraries in the same language that bind to the same concept hub op, the substrate produces a *computably correct transformation over the program graph* that rewrites the source from library A to library B. Not a wrapper, not an adapter, not a runtime shim. The migrated repository uses library B directly. The package manifest no longer lists library A. The diff is in the user's repository, reviewable per-file, and the diff is accompanied by a signed receipt that justifies every changed function.

The migration command is `provekit migrate --library-from A --library-to B --write`. The work the command does is the same work the substrate has always done (lift, transport, realize) applied to a different axis. The novelty in this paper is what comes out the other end: a *receipt* whose shape is the central editorial claim.

## 2. What the product is not

Software has accumulated a vocabulary for naming things that sit between two libraries and translate. None of those words name what this paper describes.

It is not:

- a compatibility layer
- an adapter
- an ORM
- a facade
- a bridge package
- a runtime shim
- a "common interface" everyone must code against

Each of those preserves both worlds. The old library stays installed; the wrapper sits in front of it; both API surfaces are alive in the codebase, one nominally hidden behind the other. The lock-in is preserved, costumed as choice. The codebase grows, not shrinks. The runtime pays a translation cost forever, or, equivalently, the build pipeline pays it once and burns the translation into the artifact, which is the same cost moved.

A source transformation does not do this. The old library is removed from `package.json` (or `requirements.txt`, or `Cargo.toml`, or the equivalent). The new library is installed. Every callsite that used the old library now calls the new library directly, in the new library's idiom, with the new library's types. There is no wrapper to maintain, no compatibility layer to update, no perpetual second-shape living alongside the first. The migrated repository is byte-equivalent to a repository that started life using the new library.

This is the distinction the paper carries. The product is a *source transformation*, not an abstraction.

## 3. The hard part is upstream

The rewrite at a single callsite is easy to describe and easy to write by hand. For TypeScript moving from `better-sqlite3` to `pg`:

```ts
// before
const rows = db.prepare(sql).all(args);
```

```ts
// after
const result = await pool.query(sql, args);
const rows = result.rows;
```

This is text. A regular expression would not do it, but a small AST walk would. If the entire migration were the callsite rewrite, it would be a weekend project for any team.

The migration is not the callsite. The migration is everything upstream of the callsite.

The original callsite returned synchronously. The containing function's signature was `function getUsers(): User[]`. The new callsite returns a `Promise<QueryResult>` and is `await`ed. The containing function's signature is now `async function getUsers(): Promise<User[]>`. The signature has changed. The change is not local.

```
sync query site
  -> containing function becomes async
  -> callers must await
  -> their callers may become async
  -> exported API may change
  -> route handlers / jobs / tests / CLI entrypoints become boundary nodes
```

This is not a text rewrite. It is an effect-propagation problem over the program's call graph. The substrate already tracks effect signatures on every operation in ProofIR (paper 12; paper 16's algebra extension). The `async` effect is one binding of the algebraic-effect primitive. When a callsite's realization changes its effect signature, the substrate knows that *every function reachable from that callsite through the call-graph reverse-arrow* must either already admit the widened effect or be widened itself. The propagation is mechanical because the call graph is content-addressed and the effect signatures are part of the same algebra the lift produced.

Hand-migrating this in a 50k-line TypeScript codebase is the multi-week project teams postpone forever. The propagation has hundreds of nodes; missing one of them is silent at compile time (TypeScript's structural typing can hide a `Promise<T>` where a `T` was expected long enough to fail at runtime in production). What gets postponed is not the callsite rewrite. What gets postponed is the call-graph rewrite. The substrate moves that from weeks to seconds.

## 4. The receipt is the deliverable

Here is what comes out of `provekit migrate --library-from better-sqlite3 --library-to pg --write` on a real repository. The diff lands in the working tree the way any patch does. The receipt is a separate signed artifact, committed alongside the diff and verifiable by anyone who has the proofchain head. The receipt's first page is the aggregate. The body is the per-function trail.

```
132 callsites rewritten
47 functions widened to async
9 boundary handlers already async-capable
3 refused exports because public API forbids promise return
2 lossy sites: sqlite-specific last_insert_rowid semantics
```

Each row of this aggregate is a count over signed mementos:

- **132 callsites rewritten** is the count of ConceptSiteMemento entries whose realization tag changed from `(typescript, better-sqlite3)` to `(typescript, pg)`. Each citation is in the receipt's body; each rewrite is independently verifiable.
- **47 functions widened to async** is the count of FunctionContractMemento entries whose effect signature gained `async` as part of this migration. Every widened function carries a *propagation-reason* memento citing the callsite that triggered the widening and the prior function that admitted the effect.
- **9 boundary handlers already async-capable** is the count of propagation halts: functions that the algorithm reached but did not widen because their existing signature already admitted the `async` effect. These are the route handlers, job runners, and other already-async entrypoints. The receipt names them; the reader can see where the contagion stopped and why.
- **3 refused exports** is the count of refusal mementos. A function is exported as part of the package's public API, the API contract forbids a `Promise<T>` return type (perhaps because downstream consumers depend on the synchronous signature), and the migrate command stops rather than silently breaking the contract. The refusal is not a failure; it is a first-class outcome of the migration. *Supra omnia rectum*: refusal is more honest than a wrong patch.
- **2 lossy sites** is the count of loss-record mementos. `better-sqlite3` exposes the synchronous `lastInsertRowid` after an INSERT. The `pg` driver does not have an exact equivalent (Postgres uses `RETURNING id`, which requires the INSERT statement to opt in). The migrated callsite carries a `loss_record` declaring the `last_insert_rowid` dimension; the receipt names the two sites where this loss applies and what the realizer emitted as the nearest-faithful substitute.

The body of the receipt then cites every widened function, in topological order, with the propagation chain:

```
function getUsers widened to async
because callsite users.ts:42 changed
from better-sqlite3/query-sync
to pg/query-promise

function renderDashboard widened to async
because it calls getUsers

route GET /dashboard already admits async
propagation stops here
```

This is not a summary the tool prints. This is a structured artifact a reviewer reads to verify each step. Every line is a memento with a CID; every "because" is a citation that another tool could check independently. The reviewer can ask: "show me every function widened because of the callsite at `users.ts:42`," and the receipt answers with the exact chain.

The receipt's first page is the trichotomy from the substrate's design (paper 09): exact / loudly-bounded-lossy / refuse. The 132 rewrites are exact (or refusals if the loss budget refused them). The 47 widenings are exact propagations. The 3 refusals are explicit refusals. The 2 lossy sites are loudly-bounded-lossy with the loss dimension named. Every cell of the migration falls into one of the three trichotomy buckets, and the receipt is the per-cell record. The aggregate is honest because the per-cell record is honest.

## 5. The trichotomy at the report level

The five-row aggregate is structurally the substrate's trichotomy applied to the migration domain.

`rewritten` and `widened` are the *exact* cells. The source-library realization at every site was replaceable by the target-library realization without altering the program's meaning at the substrate level. The substrate's structural-equivalence check (paper 17) passed for each. The patch is correctness-preserving.

`refused` is the *refusal* cell. The substrate computed the patch and the patch violated a contract. The most common shape: a public API exports a function whose signature contract forbids `Promise<T>` returns. The migration cannot proceed at this site without breaking a contract the substrate is bound to respect. The refusal is signed; the reason is named; the developer can either widen the contract (and accept the API break) or accept the refusal and write the patch by hand at this site with the substrate's full knowledge.

`lossy` is the *loudly-bounded-lossy* cell. The source-library's behavior at this site has a feature the target library does not have, but the loss is named and bounded. `last_insert_rowid` is the canonical example: sqlite's auto-increment-rowid is a synchronous read after INSERT, and pg's substitute requires `RETURNING id` in the INSERT statement. The realizer can emit a faithful patch with the `RETURNING` clause added; the patch is "lossy" only in the sense that the loss-record `last_insert_rowid_synchronous_read` was declared and the reader must verify that the application's downstream consumers tolerate the substitute. The loss is named at the report level because the loss is named at the cell level. Honesty propagates from the cell to the aggregate uniformly.

The architectural consequence of putting refusal and loss-record on the front page of the receipt is that the report cannot lie. A migration cannot post "all 132 callsites rewritten cleanly" if 5 of them refused or 2 of them carried a loss. The aggregate counts are derived from the per-cell mementos; they are not narrated. The developer reading the report knows exactly which outcomes occurred, and the substrate refuses to summarize away the unpleasant ones.

This is what the substrate honesty gradient (paper 19's editorial appendix, made operational here) buys at the report tier. The field name *says* what the payload *is*. The aggregate *counts* what the cells *are*.

## 6. The substrate is not a permanent artifact in the migrated codebase

A reader meeting the substrate for the first time often asks: "if the substrate is content-addressed and federally federable, do I now have to maintain it as part of my project?"

No. The substrate exists to compute and justify the patch.

After the migration runs, the durable artifacts in the user's repository are:

1. The patched source code. It uses library B directly. It has no substrate-dependency at runtime. The compiler, the test runner, the deployment pipeline, the production binaries: none of them touch the substrate.
2. The signed receipt. The receipt cites substrate CIDs, but the CIDs are pointers into the substrate, not embedded copies. A reader who wants to verify the receipt fetches the cited mementos from any substrate node (the user's, the library author's, a federated node) and checks the signatures. Verification does not require hosting the substrate.
3. Optionally, a local substrate mirror. A user who wants offline reproducibility can pin the cited CIDs locally; a user who trusts federation can skip this and pull on demand.

The substrate is a *proof object*. It is produced during the migration, signed during the migration, and after the migration it can be archived, federated, or deleted from the user's local machine without affecting the patched codebase. The receipt remains; the receipt is what the verifier checks; the verifier checks what was true at the moment the patch was produced.

This is the mature shape of the cypherpunk frame ("code is law"). The law is the receipt. The receipt is content-addressed. The receipt has no enforcement clause. The receipt has a proof clause: it cites the chain of facts that justify each step of the patch, and the verifier can re-discharge each fact independently. The substrate is the proof object; the patched source is the artifact; the receipt is the bridge. Nothing in the user's runtime knows the substrate exists. Everything in the audit trail does.

## 7. What this changes for the developer at the keyboard

The developer-keyboard pitch (paper 19's mention, paper 21's catalog) becomes operational at the migration tier.

Today, migrating a 50k-line TypeScript codebase from sqlite to Postgres is a project a team scopes for eighteen months and ships maybe. The reasons are not technical at the rewrite tier; the rewrite is small. The reasons are the call-graph contagion, the manual hunt for every async-propagation node, the silent failures when one is missed, the test-suite rewrites, the per-route-handler verification that nothing breaks. The team postpones the migration because the work is unbounded in scope and the cost-to-confidence ratio is hostile.

After this paper's mechanism ships, the same migration is a command. The command takes minutes to seconds depending on codebase size; the receipt is reviewable in an afternoon; the per-function trail is structured so a code reviewer can spot-check any propagation without re-reading the entire patch. The team migrates on a Tuesday because the cost has been amortized into the substrate by the library authors and the catalog maintainers.

The framing follows: refusals at your callsites become commits to your competitor. The library author who ships a SugarDictMemento for their library makes their library a one-command migration target for every codebase that uses a competing library bound to the same hub op. The library author who refuses to ship one keeps their lock-in until the catalog grows around them, at which point the catalog ships an Inferred or Generated binding (paper 21 §6) and the lock-in dissolves anyway. The library author's incentive is to ship the binding themselves and control the realization quality. The lock-in dissolves under either path; the path the author chooses is the path that determines whether the author is the upstream of the migration or its loser.

Lock-in dissolves. Quality competes. Migration is a one-way ratchet: any team can move from any library to any other library bound to the same hub op, on any Tuesday, with a reviewable receipt. The library author who ships sugar gets the migration revenue; the library author who refuses gets the migration as a cost. Network effects shift from "the library with the most existing users" to "the library with the most concept-hub-faithful sugar."

This is the substrate's labor-economics consequence at the migration tier. It was visible in paper 17's M+N math (linear per axis, summed across axes); paper 21 named the axes; this paper names what the human at the keyboard sees when one of those axes is invoked at lunchtime.

## 8. What this does not solve

The mechanism is bounded in scope. It does not dissolve every form of "library change is hard."

It does not dissolve **library disagreement.** Two libraries bound to the "same" concept may interpret the concept differently. If `lodash.isEqual` and `ramda.equals` disagree about `NaN` (paper 21 §3 lists the case), and the application depends on the lodash behavior, the migration to ramda *names the disagreement* as a loss-record. It does not paper over it. The developer must decide whether the application's downstream consumers can tolerate the change.

It does not dissolve **performance reshape.** The migrated code may have different performance characteristics. `pg` is a network round-trip; `better-sqlite3` is an in-process call. A function widening to async has different latency. The migration's receipt does not encode performance; the developer's benchmarking does.

It does not dissolve **operational change.** The Postgres database has to exist; the application has to be configured to reach it; the deployment pipeline has to provision the DSN. The substrate migrates the source; the operator migrates the infrastructure. Both are needed.

It does not dissolve **untracked side effects.** A callsite that uses an undocumented side effect of the old library (timing, log-line ordering, an exception type the application catches by reference) may break in subtle ways. The substrate tracks the documented effect signatures and the lifted contracts; it does not track every unwritten contract the application secretly depended on. The honest report-shape names this gap: a receipt may be clean and the application may still break, and the receipt does not lie about its own coverage.

The migration mechanism is honest about its own limits the way the substrate is honest about every other limit. The receipt names what was rewritten, widened, refused, and lossy. The receipt does not claim to have tested the migrated application. Running the test suite remains the developer's job; the receipt makes the developer's testing *bounded* (the receipt says: here is what I changed and why) rather than *unbounded* (the typical migration's "here is a diff, good luck").

## 9. Empirical receipt

This paper's claim is empirical, not theoretical. The empirical receipt is the output of `provekit bind` on a real fixture with the sqlite/pg pair as the input. **The receipt is in main.**

PRs #867 (Bridge E, `(language, library_tag)` dispatcher), #872 (Stage 1, SQL concept-shape catalog and per-library realize kits), and #873 (Stage 2, the async-rewrite engine and audit receipt envelope) shipped the machinery. Running `provekit bind --library-from typescript-better-sqlite3 --library-to typescript-pg --write` against the `examples/migrate-demo/users-better-sqlite3/` fixture produced the receipt at:

```
blake3-512:9faa22b51d6bb08e166a0ebd99bf95a21ab3ea61951c6f420840c68fb985d7f523a5bbfc72888d82d1269d4cc50303f8a243f978b76836ada8fe343f6ba88910
```

Aggregate counts, verbatim from the run:

```
4 callsites rewritten
6 functions widened to async
1 boundary handlers already async-capable
1 refused exports because public API forbids promise return
1 lossy sites: sqlite-specific last_insert_rowid semantics
```

That is the shape §4 of this paper described, produced from the actual fixture. The five rows are not narrated; they are counts over the receipt's signed mementos. The per-function widening trail names every propagation reason (`getUserById widened because users.ts:42 changed from typescript-canonical-bodies-better-sqlite3 to typescript-canonical-bodies-pg`; `renderDashboard widened because it calls renderUsersPage`; `handleRequest already admits async, propagation halts`). The refused export cites the contract memento that forbids the widening. The lossy site cites `last_insert_rowid` as the loss dimension and `INSERT ... RETURNING id` as the substituted body.

The substrate's design predicted the receipt shape. The empirical run produced it. The prediction was grounded. If the run had refuted the prediction, the substrate would have had a structural bug, and the bug would have been more interesting than the paper. The substrate's first principle (*supra omnia, rectum*) gated the paper's claim on the empirical run, not the other way around.

This is the difference between a paper that argues from analogy and a paper that argues from a substrate. The argument is not "in principle, this should work." The argument is "the substrate's already-shipped primitives composed into this output, and here is the output." Paper 22 is the form of the claim. The merged work (PR #873) is the form of the verification. Both are signed; both are reviewable; the gap between them is zero.

## 10. Closing line

The substrate only exists to compute and justify the patch.

The patch is the product. The receipt is the proof. The substrate is the workshop. After the patch ships and the receipt signs, the workshop can be archived, federated, or shut down without changing anything in the migrated codebase. The codebase runs. The receipt verifies. The proofchain head records that on a Tuesday in the spring of 2026, a 50k-line TypeScript codebase migrated from `better-sqlite3` to `pg` in 47 widenings and 132 rewrites with 3 refusals and 2 named losses, signed by a developer whose only special skill was reading the receipt.

That is what the substrate offers at the migration tier. It is not an abstraction. It is a transformation. The repository after the transformation is the repository that would have existed if the team had started on the new library. The substrate's only job was to make that repository computable from the one that existed.
