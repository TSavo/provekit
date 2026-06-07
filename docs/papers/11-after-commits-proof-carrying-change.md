# After Commits: Proof-Carrying Change as p -> q

> **Status.** Draft whitepaper. Sustained argument. Contains a theorem. Written to be cite-able after review.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [02 Bluepaper](02-bluepaper.md), [03 Substrate, not Blockchain](03-substrate-not-blockchain.md), [04 Vertical Stack and Standardization](04-vertical-stack-and-standardization.md), [05 Witness Pluralism and Jurisdiction-Neutral Transport](05-witness-pluralism-and-jurisdiction-neutral-transport.md), [06 After Reputation](06-after-reputation-software-as-federated-truth-claims.md), [07 After Verification](07-after-verification-bug-classes-as-missing-edges.md), [08 After Types](08-after-types-stop-logging-trust-the-invariant-solver.md), [09 Lossy Boundary Compression](09-lossy-boundary-compression.md), and [10 After Protocol Specs](10-after-protocol-specs-how-protocols-actually-evolve.md).
>
> **Protocol companions.** `protocol/specs/2026-05-06-fix-receipt-protocol.md`, `protocol/specs/2026-05-06-obligation-realizer-protocol.md`, `protocol/specs/2026-05-06-truth-discharge-protocol.md`, and `protocol/specs/2026-05-06-extension-protocols.md`.
>
> **Premise the earlier papers established.** A protocol for content-addressable, cryptographically-signed, byte-deterministic claims about software behavior, federated across signers, composable end-to-end, jurisdiction-neutral, machine-checkable, and deliberately lossy at the contract-boundary layer. Paper 07 argued that bug classes become missing edges. Paper 09 argued that ProofIR is universal because it compresses contract boundaries. Paper 10 argued that protocols themselves become witnessed DAGs.
>
> **What this paper argues.** That software change itself has the same shape. A commit can become a claim-bearing transition from parent proof state `p` to child proof state `q`. The `.proof` root is the typed claim root for that transition: preservation receipts for obligations that survive, fix receipts for obligations that changed, and refusal receipts for what the substrate cannot know. With edge compression, the whole proof tree travels as `p -> q`.

## Section 0: The claim

A git commit is one of the most successful data structures in software history.

Its shape is small:

```text
tree
parent(s)
author/committer metadata
message
```

That shape solved a real problem. It made source history content-addressed enough to distribute, compare, merge, sign, bisect, review, and recover. The commit object names bytes, ancestry, and human intent. It does not name semantic consequence.

That omission was reasonable when the available machine-checkable objects were mostly bytes, trees, patches, tests, and signatures. A commit could say:

```text
fix null dereference in UserDirectory
```

but the object itself did not carry the proof of that sentence. The proof lived elsewhere: in reviewer judgment, CI results, unit tests, a static analyzer warning disappearing, a bug report closing, a release note, a CVE advisory, a security engineer's memory, or nothing at all.

Sugar changes the possible shape.

The semantic commit is:

```text
tree
parent(s)
message
.proof root
```

That fourth field is the change. The `.proof` root is not a decoration and not a log. It is the typed claim root for the transition this commit makes.

The parent commit has a proof state:

```text
p
```

The child commit has a proof state:

```text
q
```

The commit's load-bearing claim is:

```text
p -> q
```

The diff may be large. The proof tree may be large. The review discussion may be long. But the semantic transition compresses to an implication edge: given the parent proof state `p`, this commit produces child proof state `q` under named policy, with receipts for every nontrivial edge.

That is the claim:

**A commit becomes a proof-carrying transition.**

Not "trust my diff." Not "CI passed." Not "an LLM said it fixed the bug." Not "the reviewer was careful." The commit carries the typed evidence for what changed, what did not change, and what the substrate explicitly refuses to know.

This is not a claim that every behavior of every program is now proven. It is a claim about object shape. Today, a commit is a content-addressed byte transition with a human message. In the substrate, a commit can also be a content-addressed semantic transition with a machine-checkable proof root.

The difference is small at the data-structure level and enormous at the trust boundary.

## Section 1: A diff is not a claim

A diff is evidence that bytes changed.

It is not, by itself, evidence that behavior was preserved, a vulnerability was fixed, an invariant became stronger, a migration is safe, or an API contract remained compatible.

Consider these ordinary commit messages:

```text
refactor auth middleware
fix SQL injection
upgrade OpenSSL
rename payment field
make parser stricter
support HTTP/2
remove dead code
```

Each message is a semantic claim.

The diff does not type the claim. It does not say which obligations were supposed to be preserved. It does not say which missing edge was closed. It does not say whether the change is a behavioral strengthening, behavioral weakening, refactor, migration, compatibility bridge, or refusal. It does not say which proof policy admitted the result. It does not say which lifter produced the boundary object. It does not say where the substrate could not see.

Reviewers infer all of this.

CI infers a small part of it by running selected tests. Static analysis infers another small part by checking selected patterns. Code owners infer another part by knowing the system. Security teams infer another part by comparing the patch to a known exploit. Release managers infer another part by reading the changelog. The commit object itself remains silent.

This silence creates three pathologies.

First, review is overloaded. A human reviewer must inspect syntax, style, architecture, tests, compatibility, security, performance, and semantic consequence at once. The reviewer is asked to answer "is this right?" when the artifact does not say which semantic proposition it is asking to have accepted.

Second, automation is underspecified. CI can say "these commands passed." It cannot say "this commit preserves the obligation set named by the parent root, except for the two obligations intentionally changed by these receipts." The CI pipeline may approximate that answer with tests and analyzers, but the result is not the commit's typed claim.

Third, history becomes forensic. When a later bug appears, engineers mine old commits and ask what a change meant. Did this commit intentionally weaken validation? Did it accidentally remove a boundary check? Was the missing edge ever closed? Did the fix that went into the code correspond to the security advisory's actual obligation? The repository has bytes and prose, but not the semantic receipt.

This is why "vulnerable and fixed" is the wrong primitive.

The useful primitive is:

```text
vulnerable -> exposed -> dropped
```

The exposed version has a missing edge. The dropped version is not accepted because it resembles a human patch. It is accepted because the generated or authored change re-lifts to ProofIR and closes the named edge under policy. The important artifact is not the fix as style. The important artifact is the receipt that makes the semantic transition checkable.

A diff is a byte delta. A proof-carrying commit is a typed semantic delta.

## Section 2: Proof state

The phrase "commit proof state" needs a narrow definition.

It does not mean the complete behavior of the program. It does not mean every possible trace. It does not mean the entire mathematical semantics of the repository. It means the set of proof roots and obligation roots that local policy admits for the repository at that commit.

For a parent commit, policy may admit:

```text
obligationSetCid
contractBoundaryRootCid
foundationCatalogCid
liftProfileCid
preservationReceiptRootCid
fixReceiptRootCid
refusalReceiptRootCid
testWitnessRootCid
conformanceWitnessRootCid
  -> parentProofStateCid
```

For a child commit, policy admits another root:

```text
obligationSetCid'
contractBoundaryRootCid'
foundationCatalogCid'
liftProfileCid'
preservationReceiptRootCid'
fixReceiptRootCid'
refusalReceiptRootCid'
testWitnessRootCid'
conformanceWitnessRootCid'
  -> childProofStateCid
```

The commit claims a transition:

```text
parentProofStateCid -> childProofStateCid
```

Call the parent state `p` and the child state `q`.

The commit-level edge is:

```text
p -> q
```

This is the same substrate move that appeared in earlier papers:

- a bug is a missing edge;
- a protocol upgrade is a witnessed edge from old protocol root to new protocol root;
- a lifter translation is an edge from host-language boundary to ProofIR boundary;
- a fix is an edge from vulnerable boundary state to closed boundary state.

The commit is just the place where these edges become the unit of everyday software change.

The proof state is policy-relative. Two organizations may accept different witness sets. A safety-critical deployment may require stronger receipts than an internal tool. A language team may trust one lifter and refuse another. A security team may require a specific foundation catalog. A maintainer may accept test witnesses for low-risk refactors but require solver witnesses for security fixes.

This policy relativity is not a defect. Git already has local policy: branch protection, required reviews, signed commits, required checks, allowed merge strategies, CODEOWNERS, release gates. The substrate makes semantic policy explicit and content-addressed.

The proof state is also lossy. It preserves contract boundaries, obligations, receipts, and refusals. It does not preserve implementation texture unless that texture has been lifted as an obligation. This is paper 09 applied to commits: the commit proof root is not a universal re-expression of the repository. It is a universal boundary claim over the change.

That narrowness is why the object can travel.

## Section 3: `.proof` as typed claim root

The `.proof` root is the typed claim root attached to a change.

A minimal commit proof body may include:

```text
kind = CommitProof
schemaVersion = 1
commitTreeCid
parentCommitCid(s)
messageCid
diffCid
parentProofStateCid
childProofStateCid
preservationReceiptRootCid
fixReceiptRootCid
refusalReceiptRootCid
policyCid
producerCid
signatureCid
```

The exact wire form belongs in a protocol spec. The paper-level point is simpler: the `.proof` root binds the byte object to the semantic object.

Without `.proof`, the commit has bytes and prose.

With `.proof`, the commit has bytes, prose, and typed evidence.

The evidence has at least three receipt classes:

```text
preservation receipts: obligations claimed to survive
fix receipts: obligations intentionally changed or closed
refusal receipts: obligations the substrate cannot know
```

These are not merely labels. They are different kinds of claim.

A preservation receipt says:

```text
the parent obligation remains true in the child state
```

A fix receipt says:

```text
the parent state had a named gap, the child state closes it,
and the closure is admitted under this policy
```

A refusal receipt says:

```text
the substrate does not claim this edge
```

The refusal is as important as the positive receipts. Silent unknowns are where trust leaks. An explicit refusal is a boundary: this lifter does not model that reflection path; this proof policy does not accept that solver; this test witness samples but does not prove that temporal property; this source artifact contains generated code whose origin is not available; this platform call crosses into an unsupported kernel contract.

An honest `.proof` root with refusals is more useful than a broad green check with hidden blindness.

The object can be bound to Git in several ways before Git itself changes:

- a tracked `.proof` file whose root is included in the tree;
- a signed commit trailer naming the proof root;
- a git note over the commit object;
- a release attestation that binds commit CID to proof root;
- a future native commit-object extension that includes the proof root directly.

Those are deployment choices. The semantic shape is the same:

```text
commit bytes + proof root -> claim-bearing commit
```

Once the proof root is bound, the commit is no longer just a snapshot. It is a proposition about a transition.

## Section 4: Preservation receipts

Most commits claim sameness.

The message might not say it, but the claim is there. A refactor says "same behavior, clearer structure." A rename says "same behavior, different name." A formatting change says "same behavior, different text." A dependency upgrade often says "same public obligations, new implementation." A performance optimization says "same results, different cost model." A migration says "same accepted API contract, new storage representation."

Every one of those claims is currently informal unless the repository has a domain-specific proof or test suite that captures it.

A preservation receipt gives the claim a shape:

```text
parentObligationCid
childObligationCid
translationCid
equalityOrImplicationWitnessCid
policyCid
  -> preservationReceiptCid
```

There are several preservation modes.

**Exact preservation.** The lifted child obligation has the same canonical ProofIR CID as the parent obligation. This is the simplest case:

```text
parentObligationCid == childObligationCid
```

**Equivalent preservation.** The child and parent obligations are different expressions but mutually imply each other under policy:

```text
parentObligationCid -> childObligationCid
childObligationCid -> parentObligationCid
```

**Strengthening preservation.** The child obligation is stronger. This preserves callers who depended on the parent obligation but may reject more inputs:

```text
childObligationCid -> parentObligationCid
```

Strengthening is not always backward compatible, but it is semantically typed. A parser becoming stricter may be a security fix or a breaking change depending on API policy. The receipt does not decide the business policy; it gives the policy something precise to decide over.

**Projection preservation.** The child and parent differ in implementation texture, but the projection relevant to the boundary domain is unchanged. This is paper 09's lossy boundary compression:

```text
project(parentBoundary) == project(childBoundary)
```

These modes are how "same behavior" stops being a vibe.

The preservation receipt says exactly which behavior, at which boundary, under which equivalence relation, admitted by which policy.

That matters for review. A reviewer looking at a refactor should not have to infer the entire semantic preservation claim from a diff. The commit can say:

```text
I changed these files.
I preserved these obligations.
Here are the preservation receipts.
Here are the refusals.
```

The reviewer can then spend attention on the places where the proof root is absent, weak, surprising, or policy-dependent. That is a better use of human judgment than rereading mechanical sameness.

## Section 5: Fix receipts

A fix receipt is the receipt for changed behavior.

It does not say "the patch looks like a fix." It says:

```text
there was a named missing edge,
candidate bytes were produced,
the candidate bytes were re-lifted,
the post-lift ProofIR closes the missing edge,
and the closure witness is admitted under policy
```

The minimal shape from the Fix Receipt Protocol is:

```text
gapCid
missingEdge
planCid
preArtifactCid
patchCid
transformedArtifactCid
postLiftCid
closureWitnessCid
policyCid
producer
  -> fixReceiptCid
```

The nontriviality rule is the heart of the object:

```text
preArtifactCid != transformedArtifactCid
postLiftCid exists
closureWitnessCid exists
closureWitnessCid discharges gapCid under policyCid
```

A fix receipt is not the fix. It is the content-addressed claim that the fix closed the edge.

This distinction is load-bearing.

The code that actually lands may differ from the patch a human would have written. The implementation style may be uninteresting. The variable names may be boring. The guard may be expressed as a native Sugar annotation, a Spring annotation, a schema constraint, a validator, a generated wrapper, a type-level refinement, a migration check, or a lower-level resource-state transition. For the substrate, the question is not "is this the canonical human patch?" The question is:

```text
does the post-lift boundary state close the edge?
```

That is why droppers matter.

The dropper does not patch code in the old sense. It realizes a missing obligation into a host artifact and then has to survive re-lift. The dropped version is accepted only if the substrate can see the closed edge.

For the commit, the fix receipt is the typed permission slip for semantic change.

Without it, a commit that says "fix X" is only a candidate. With it, the commit carries the evidence that X, as named by a ProofIR gap, was closed.

## Section 6: Refusal receipts

The substrate must be allowed to say no.

It must also be allowed to say "I do not know."

That second sentence is where many verification systems become dishonest. They present a green result for the portion they modeled, while the unmodeled portion becomes ambient trust. Reflection, dynamic dispatch, generated code, native calls, time, randomness, network behavior, undefined behavior, platform APIs, serialization quirks, framework magic, and deployment configuration all become places where the tool quietly stops seeing.

A proof-carrying commit cannot hide those holes.

A refusal receipt says:

```text
refusedEdgeCid
reasonCid
lifterCid
artifactCid
policyCid
scopeCid
  -> refusalReceiptCid
```

Examples:

```text
unsupported dynamic reflection path
native call boundary requires external axiom
test witness samples this path but does not prove it
lifter cannot model framework plugin side effects
solver timeout under this policy
generated artifact origin unavailable
cryptographic constant-time behavior not lifted by this profile
```

Refusal is not failure. Refusal is explicit shape.

A commit with refusals may still be accepted. Local policy decides. A documentation change may tolerate broad refusals. A production security fix may not. A safety-critical release may require every refusal to be waived by a human authority key. An experimental branch may allow weaker witnesses.

The point is that the commit can no longer pretend.

The reviewer sees the positive receipts and the refusals in the same typed root. That makes the proof root useful even when it is incomplete. Especially when it is incomplete.

An incomplete proof root that names its incompleteness is a better artifact than a CI green check that does not.

## Section 7: Edge compression

The proof tree can be large.

A real commit may touch many files, lift many boundaries, preserve many obligations, close several gaps, introduce new contracts, refuse unsupported domains, and rely on external witnesses. Expanding every proof tree in every review would be impossible.

The substrate does not require that.

The substrate is content-addressed. The commit travels with roots.

At the top level:

```text
p -> q
```

Inside that edge:

```text
preservationReceiptRootCid
fixReceiptRootCid
refusalReceiptRootCid
policyCid
  -> commitTransitionWitnessCid
```

Inside a fix receipt:

```text
preArtifactCid
transformedArtifactCid
postLiftCid
closureWitnessCid
policyCid
  -> fixReceiptCid
```

Inside a closure witness:

```text
antecedentPredicateCid
consequentPredicateCid
solverWitnessCid
checkerCid
  -> implicationWitnessCid
```

The whole tree is expandable, but the commit only needs to carry the root and the reachable CIDs. A consumer expands as much as local policy requires.

That is edge compression.

The important relation is not "here are all proof leaves printed in the commit message." The important relation is:

```text
parent proof state p implies child proof state q under these receipts
```

The root is small. The evidence is addressable. The expansion is demand-driven.

This is why the commit can become proof-carrying without becoming unreadable.

Review can happen at multiple depths:

```text
depth 0: root present and signed
depth 1: receipt classes match policy
depth 2: all changed obligations have receipts
depth 3: all receipts re-check locally
depth 4: all witnesses re-run from source artifacts
depth 5: all external axioms traced to accepted authorities
```

Different users choose different depth. The root is the same.

This is paper 05's jurisdiction-neutral transport applied to commits: the proof root can travel through GitHub, email, tarballs, package registries, mirrors, USB drives, IPFS, artifact stores, or release attestations. Transport is not trust. The root is the trust-bearing object.

## Section 8: Git binding

Git already content-addresses commits, trees, blobs, and tags.

It also already supports signatures, notes, trailers, and external attestations. Those mechanisms can bind a `.proof` root to a commit before any native Git object change exists.

The ideal object is:

```text
tree
parent(s)
message
proofRoot
  -> commitCid
```

Vanilla Git does not have that fourth field in the commit object. The transitional binding is:

```text
gitCommitCid
proofRootCid
bindingMode
signatureCid
  -> commitProofBindingCid
```

The binding can be stored as:

- a tracked `.proof` file in the tree;
- a signed trailer in the commit message;
- a git note signed by an accepted key;
- a release attestation;
- a CI-produced artifact pinned by CID;
- a future native object format.

Each has tradeoffs.

A tracked `.proof` file is reviewable and travels with normal repository operations, but it changes the tree and may create merge conflicts. A trailer is simple and human-visible, but weak if not signed and structured. A git note avoids changing the tree, but notes are less consistently transported. A release attestation fits supply-chain workflows, but may arrive after the commit. A native object format is cleanest, but requires ecosystem adoption.

The paper does not depend on one storage choice. It depends on binding strength:

```text
commit bytes and proof root must be signed or content-bound together
```

Once bound, the commit can be checked as a semantic transition.

There is an apparent chicken-and-egg problem:

```text
if the proof root is inside the commit tree,
then the commit hash changes when the proof root is written;
but the proof root wants to bind the commit hash
```

CI solves the bootstrap.

The first production deployment does not need the proof root inside the commit object. It needs a signed binding over an ordinary commit:

```text
ordinaryGitCommitCid
proofRootCid
ciRunCid
policyCid
builderIdentityCid
  -> CommitProofBindingCid
```

The developer pushes normal Git commits. CI checks out the exact commit CID, lifts the tree, computes the proof root, validates receipts, signs the binding, and publishes the binding as a required check artifact or attestation. Branch protection requires the binding before merge.

That makes CI the first proof-authority:

```text
push ordinary commit
CI computes .proof root
CI signs CommitProofBinding(commitCid, proofRootCid)
branch policy admits or refuses
```

No recursive hash problem appears because the binding is outside the commit object. The commit CID names the bytes. The CI-produced binding names the proof root over those bytes. A later merge commit, release tag, git note, or native object format can carry the binding forward.

If a project wants the proof material tracked inside the repository, the repository can add a second, proof-materialization commit:

```text
payload commit changes code
CI mints CommitProofBinding(payloadCommitCid, proofRootCid)
proof-materialization commit records that binding under .proof/
```

That second commit is aesthetically ugly but operationally solvable. It is a bookkeeping commit, not the semantic repair. Its program-behavior claim should be preservation: source obligations are unchanged, and the proof index gained a binding for an earlier payload commit. Repositories that dislike the extra commit can use git notes, release attestations, CI artifacts, or signed external ledgers instead.

The important rule is that the authoritative binding need not be stored inside the tree whose CID it binds. For bootstrap, the binding lives one layer above the commit. Native commit-object support can later move the proof root into the object itself.

This is the same transition path as many supply-chain systems. First the artifact exists. Then a builder signs an attestation over it. Later, ecosystems learn to require and transport that attestation as if it had always been part of the artifact's shape.

The eventual native object is still clean:

```text
tree
parent(s)
message
proofRoot
  -> commitCid
```

But the adoption path starts one layer above Git:

```text
commitCid + CI-signed proof binding
```

That is enough to make protected branches semantic today.

The bootstrap inherits the lifter's correctness. A `CommitProofBinding` signed by CI binds whatever the lifter produced over the tree, including its bugs. Refusal receipts catch known unknowns by design; lifter defects are unknown unknowns and cannot refuse themselves. The empirical answer is the Bug Zoo loop: lifter regressions surface as missing rediscovery edges, and a lifter that fails to recover known historical obligations is wrong, not refining. The bootstrap does not eliminate trust in the lifter. It makes that trust explicit, named by lifter CID and profile, and policy-checkable. A consumer that does not trust a particular lifter can refuse bindings produced by it, and a repository that wants stronger guarantees can require multiple lifters to converge before the binding is admitted.

Branch protection can evolve from:

```text
require signed commits
require CI checks
require review
```

to:

```text
require signed commits
require CI checks
require review
require commit proof root
require nontrivial fix receipts for claimed repairs
require preservation receipts for protected obligations
require explicit refusals for unsupported domains
```

This does not remove review. It gives review a typed object.

The reviewer is no longer asked to infer every semantic claim from bytes. The reviewer is asked to inspect whether the commit's proof root tells the truth, whether the receipts are strong enough, whether the refusals are acceptable, and whether local policy should admit the transition.

That is a better question.

## Section 9: LLM commits

LLMs make this urgent.

A human patch can be wrong. An LLM patch can be wrong at industrial scale.

The old mitigation is process: better prompts, better review, smaller diffs, tests, static analysis, code owners, model selection, sandboxing. All of those help. None changes the object shape.

A model can still produce a plausible patch with a plausible explanation and a plausible commit message:

```text
fix null handling in user lookup
```

The prose is not evidence.

In the substrate, an LLM is a candidate generator. It may propose code, schemas, annotations, migrations, tests, or native contracts. The commit is accepted only if the candidate attaches receipts.

The rule is:

```text
LLM output without receipt = candidate
LLM output with nontrivial fix receipt = witnessed semantic change
```

This moves the trust boundary.

The model does not have to be trusted to know whether it fixed the bug. The model has to produce a candidate that survives lift, verification, and receipt validation. Its useful role is search. The substrate is the acceptance function.

The same applies to preservation.

If a model refactors code, it should not merely say "behavior preserved." It should attach preservation receipts for the protected obligations or refusals for the parts it cannot preserve. A model that cannot produce those receipts can still be useful, but its output is a candidate awaiting proof, not a proof-carrying commit.

This is the governance shape every AI coding system will need if it wants to modify serious codebases:

```text
model proposes
dropper realizes
lifter re-lifts
verifier checks
receipt binds
commit carries
policy admits or refuses
```

The model's confidence score is not on that list.

## Section 10: Review, CI, and release

Proof-carrying commits do not replace the existing engineering stack. They change what the stack is checking.

**Review.** Review becomes semantic triage over typed claims. The reviewer asks: Are the changed obligations the ones the author claims? Are the preservation receipts credible? Are the fix receipts nontrivial? Are the refusals acceptable? Did policy admit too much?

**CI.** CI becomes proof-root construction and verification, not only command execution. It may run tests, lifters, solvers, conformance checks, receipt validators, and policy gates. The output is not merely "passed." The output is a content-addressed witness root.

**Release.** A release becomes a higher-level proof transition over many commits. If each commit carries `p_i -> p_{i+1}`, the release root can compress the chain:

```text
p_0 -> p_n
```

Release notes can then name not only features and fixes, but the obligation deltas:

```text
closed edges
preserved protected obligations
new refusals
weakened or deprecated contracts
migration receipts
```

**Backporting.** A backport is no longer "apply a similar patch to an older branch and hope it fixes the same issue." It is "realize the same missing edge against an older proof state and require a new fix receipt." The patch may differ. The closed edge should match.

**Bisecting.** Today, `git bisect` finds the byte transition that introduced a failing test. With proof roots, bisect can search semantic state: when did obligation `O` disappear, weaken, or become refused? The answer is a commit transition, not merely a file diff.

**Dependency updates.** A dependency upgrade can be checked as a transition between dependency proof states. The application does not merely trust "version 2.3.4." It asks whether the dependency's new proof root preserves or strengthens the obligations the application depends on.

**Security advisories.** A CVE fix can publish the missing edge and the accepted fix receipts. Downstream consumers can ask whether their branch has closed the same edge, even if their patch differs from upstream. This is the difference between "contains the fix commit" and "closes the vulnerability obligation."

This is where CVE archaeology changes.

The question stops being:

```text
did this repository cherry-pick commit abc123?
```

and becomes:

```text
does this repository's proof state close edge E?
```

That is a strictly better question.

## Section 11: The theorem

**Theorem (Proof-Carrying Commit Transition).** Let `C_p` be a parent commit with admitted proof state `p` under local policy `Policy`. Let `C_q` be a child commit with tree, parent pointer to `C_p`, message, and a bound `.proof` root. Let the `.proof` root contain preservation receipts, fix receipts, refusal receipts, and a transition witness root. If every protected parent obligation is either preserved, intentionally transformed by an admitted fix receipt, or explicitly refused under `Policy`, and if every receipt binds its referenced artifacts by CID and verifies under `Policy`, then `C_q` represents a machine-checkable semantic transition `p -> q` for the obligation domain named by `Policy`.

Equivalently:

```text
all protected obligations accounted for
all changed obligations justified
all unknown obligations refused
all receipts content-bound and policy-admitted
  => commit admits p -> q
```

**Proof sketch.** The parent proof state `p` is a content-addressed root over obligations admitted at `C_p`. The child proof state `q` is a content-addressed root over obligations admitted at `C_q`. For each protected obligation in `p`, the `.proof` root supplies one of three typed witnesses: a preservation receipt, a fix receipt, or a refusal receipt. Preservation receipts witness that the obligation survives into `q` under the relevant equivalence or implication relation. Fix receipts witness that a named missing or changed edge is closed by re-lifted child artifacts under policy. Refusal receipts remove the obligation from the positive claim and make the absence explicit under policy. Since each receipt binds its artifacts by CID and verifies under `Policy`, the transition witness accounts for the complete protected obligation set. Therefore local verification can accept the edge `p -> q` without trusting the commit message or producer identity as semantic evidence. QED.

The theorem is scoped. It says "for the obligation domain named by `Policy`." It does not say every possible behavior is proven. It says the commit's semantic claim is explicit, finite, content-addressed, and checkable.

That is enough to change the engineering primitive.

## Section 12: Counterarguments

**"This is too expensive for every commit."** Only if every verifier expands every tree every time. The substrate is content-addressed. Existing receipts cache. Unchanged obligations preserve by CID. Review expands by policy and risk. Most commits compress to preservation roots and a small refusal set. Expensive witnesses amortize across repositories.

**"Most commits are trivial."** Trivial commits are where explicit preservation is cheapest. Formatting changes should have small proof roots: same lifted obligations, same protected set, maybe no fix receipts. A trivial commit with no proof root is still asking the reviewer to infer triviality.

**"Git already has signed commits."** A signed commit tells you who signed bytes. It does not tell you what semantic claim the bytes make. Proof-carrying commits do not replace signatures; they give signatures a semantic object to sign.

**"CI already proves the commit works."** CI proves selected commands passed in selected environments. It is valuable. It is not the same as accounting for protected obligations, changed edges, and refusals. In the substrate, CI can become one producer of witnesses inside the proof root.

**"Not all behavior can be lifted."** Correct. That is why refusal receipts are first-class. The honest proof root says what it cannot know. Policy decides whether that is acceptable.

**"Receipts can be shallow or gamed."** So can tests, reviews, static analysis, and commit messages. The difference is that a receipt has a typed nontriviality rule and content-bound references. A shallow receipt can be rejected by policy, challenged by local re-checking, or replaced by a stronger witness.

**"This will slow developers down."** It will slow down unaccounted semantic change. That is the point. It should make mechanical preservation cheaper, security fixes clearer, LLM output safer, and release review less forensic. The friction moves from post-incident reconstruction to pre-merge accounting.

**"The proof root could be wrong because the lifter is wrong."** Yes. Lifters are part of the trusted or policy-admitted base. This is not hidden. The proof root names lifter CIDs, profiles, policies, and refusals. A wrong lifter becomes a wrong signed artifact that can be revoked, replaced, or refused. Today, the equivalent error is usually invisible.

## Section 13: Consequences

The first consequence is that commit messages lose their load-bearing semantic role.

They remain important for humans. A good message explains intent, context, and rationale. But the message no longer has to carry the proof. The `.proof` root carries the proof.

The second consequence is that code review becomes less theatrical. Reviewers still matter, but they are no longer asked to pretend that reading a diff is the same thing as checking every semantic consequence. They review the proof boundary, the receipts, the refusals, and the policy fit.

The third consequence is that AI-generated code becomes governable. The question is not "which model wrote this?" but "which receipts does this change carry?" A weak model that finds a receipt-valid patch is acceptable. A strong model that cannot attach receipts produced a candidate.

The fourth consequence is that supply-chain integrity stops at a better place. SLSA-style provenance says where an artifact came from and how it was built. A proof-carrying commit says what semantic transition occurred. The two compose:

```text
source provenance
build provenance
commit proof transition
release proof transition
  -> supply-chain claim root
```

The fifth consequence is that vulnerability management becomes edge-based. A fix is not a commit hash. A fix is a closed missing edge. Commit hashes are one way the edge may have been closed in one repository at one time.

The sixth consequence is that software history becomes queryable by obligation.

Today:

```text
show me commits touching auth/
show me commits mentioning CVE-2026-...
show me commits by Alice
show me commits that changed UserDirectory.java
```

With proof roots:

```text
show me when non_null(name) became protected
show me every commit that refused constant-time crypto obligations
show me which release closed untrusted -> safe_for_sql
show me all commits that weakened parser strictness
show me dependency upgrades that preserved my API obligations
```

That is not a nicer log format. It is a different history object.

## Section 14: The end of "trust my diff"

The old commit asked for trust in several places at once.

Trust the author. Trust the reviewer. Trust the tests. Trust the CI environment. Trust the static analyzer configuration. Trust the commit message. Trust that the diff means what it appears to mean. Trust that the omitted details are irrelevant. Trust that the patch that fixed upstream also fixes your branch. Trust that the LLM did not hallucinate a subtle weakening. Trust that the unsupported framework path does not matter.

The proof-carrying commit does not eliminate trust. It names it.

It says:

```text
Here is the parent proof state.
Here is the child proof state.
Here is the transition edge.
Here are the obligations preserved.
Here are the missing edges closed.
Here are the fix receipts.
Here are the refusals.
Here is the policy.
Here are the content roots.
Here are the signatures.
```

That is the jaw-drop moment.

The commit becomes a claim-bearing object. The `.proof` root is the typed claim root. Preservation receipts carry "same behavior." Fix receipts carry "changed behavior." Refusal receipts carry "the substrate cannot know." Edge compression lets the whole proof tree travel as:

```text
p -> q
```

The rest is expansion on demand.

Software history has always been a sequence of changes. With proof-carrying commits, it becomes a sequence of witnessed semantic transitions.

That is what comes after commits.
