# API-tier concept tagging: how cross-library transport dissolves without a translation matrix

The earliest concept mints in ProvekIt's catalog described primitive operations: `concept:add`, `concept:conditional`, `concept:option`, `concept:linear-iteration`. Those got us across languages by giving each language's primitive its own CID, namespaced by language, and recording a discharged morphism to a shared concept-hub CID. Paper 13 names the algebra; paper 17 names the address space; paper 18 names the hub. Cross-language transport at the operation tier is M+N, not M×N, because the hub mediates.

The same mechanism handles cross-library transport, but only if the catalog admits API-tier concepts on equal footing with primitive-tier ones. The general claim, sharpened by Sir, 2026-05-13:

> Cross-platform library behavior dissolves when the library API is recognized and tagged as a high-level concept realization. The library is not a special layer. It is more terms in the same algebra, with CIDs at every level.

This doc says, plainly, what that rule is, what the admission paths are, and what happens when a target language has no realization for a concept the source program needs. It does not introduce new substrate primitives. The primitives already exist; this is a curation rule about what gets tagged.

## The rule

A high-level library call is a realization of a `concept:*` CID, not a member of a special library layer.

That sentence is short on purpose. The implications:

1. `_.isEqual(a, b)` (lodash, JavaScript) and `std.meta.eql(a, b)` (Zig stdlib) and `a == b` (Python on hashable structures) and `a.eq(&b)` (Rust core) and `Objects.deepEquals(a, b)` (Java stdlib) are five surfaces over the same hub CID: `concept:deep-equality`. The lifter recognizes the call. The catalog records the morphism. The realize step picks the surface for the target.
2. `fetch(url)` (JS) and `requests.get(url)` (Python) and `reqwest::get(url)` (Rust) and `libcurl` (C) and `java.net.http.HttpClient` (Java) and `URLSession.shared.dataTask(with:)` (Swift) are surfaces over `concept:http-request`. Same mechanism. Same M+N.
3. `JSON.stringify(x)` and `json.dumps(x)` and `serde_json::to_string(x)` and `Gson().toJson(x)` are surfaces over `concept:json-encode`. Same mechanism.
4. lodash's `debounce`, the framework-native schedulers, the hand-rolled `setTimeout`+`clearTimeout` pattern, and the Rust crate `tokio::time::sleep`-with-cancel pattern are surfaces over `concept:debounce`.

The catalog does not need a separate "library tier" with its own protocol. It needs more CIDs, all minted the same way, all sitting in the same federated table. The hub does the same work it has always done. Naming the rule explicitly closes the question "do we add a library layer?" The answer is no; the layer is already there.

## What this is not

It is not a claim that every library call has a meaningful concept. Some library calls are tightly coupled to the surrounding ecosystem (a React hook with React-specific scheduling semantics is not just a generic effect handler). Those bind to narrower concepts, or to no concept, and the loss-record carries the un-portable bits. The rule says "tag what is generic." It does not say "force everything to be generic."

It is also not a translation matrix. There is no M×N table of library pairs. Each library surface gets one lift entry and one realize entry per concept it binds. The catalog table is indexed by `(concept, language, library)` triples, not by `(source_library, target_library)` pairs. Two surfaces transport through the hub or they refuse; they do not transport pairwise.

It is also not a replacement for primitive-tier concepts. Both tiers live in the same catalog. A lifted Rust program may carry `concept:add` (primitive), `concept:linear-iteration` (control), and `concept:http-request` (API) in the same `(V, A, C, ≤)` tuple. The address space is flat; the tier is a property of the concept's contract, not of its address.

## The first API-tier concept backlog

These are the concepts on deck for minting, in priority order. Each will get its own issue under #845 as work is dispatched:

1. `concept:http-request` and `concept:http-response` (in flight as #847, trinity-receipt prereq)
2. `concept:deep-equality`
3. `concept:json-encode`, `concept:json-decode`
4. `concept:regex-match`
5. `concept:retry-with-backoff`
6. `concept:debounce`

The ordering reflects audience cross-section: HTTP and JSON touch every working developer; regex and retry land soon after; debounce closes the most-asked-about UI primitive. Other concepts (logging, metrics, tracing, key-value cache, queue producer/consumer, file IO, env-var read, command-line argument parse, datetime formatting, locale-aware sort, content negotiation, OAuth flow, JWT verify, password hash) are catalog-track but not on the priority list yet. The backlog grows monotonically; nothing here is exclusive.

## The four admission paths for an API-tier binding

A `concept-binding-claim` memento says "this surface artifact is a realization of this concept CID." A claim becomes part of the catalog through one of four paths, distinguished by who minted it and what discharge it carries.

### Path 1: Authored

A human declares the binding. The memento is signed by the human's key, content-addressed, and submitted to the catalog via PR. The verifier's policy chooses whether to accept the signature alone or to require an additional discharge step.

This is the bedrock path. The first batch of API-tier concepts (HTTP, JSON, regex, deep-equality) will be Authored. The catalog's gold-standard cells become the discharge ground truth for the other three paths.

### Path 2: Inferred

The substrate, given two surface artifacts and their lifted `(V, A, C, ≤)` tuples, recognizes that the tuples are structurally equivalent up to a renaming morphism. The recognition is a CID comparison after applying the renaming; the morphism is content-addressed; the equivalence claim is a discharged fact, not an assertion.

Inferred bindings grow the catalog as lifters process more code. A `concept-binding-claim` produced by inference carries the discharge proof inline. The human review step is "accept this candidate?" not "is this true?"

### Path 3: Generated

A language model proposes the binding. The model has read documentation and example code; it asserts `lodash.isEqual` and `ramda.equals` are surfaces over the same concept. The proposal becomes a `concept-binding-claim` with `source: llm-generated` and an explicit confidence basis.

The substrate then runs the discharge: lift both surfaces, check structural equivalence after applying the proposed renaming morphism. If the equivalence holds, the claim is accepted; if it does not, the claim is refused with a `lossy-realization-refusal` carrying the precise structural divergence.

#841 (GenerativeCompletionProtocol) tracks this path. Generated bindings eat the long tail of the catalog: the cells humans will never get around to authoring by hand.

### Path 4: Self-Attested

The library author signs the binding themselves and publishes it alongside their package release. `npm publish` ships the package and a `provekit-sugar.json` (or equivalent convention) declaring the surface-to-concept mapping. The trust profile is identical to trusting the library itself: the signature on the binding is the existing release key.

This is the scaling path. The other three paths bottleneck on curator-hours, lifter coverage, or LLM compute. Self-Attested bottlenecks on world publication rate, which is several orders of magnitude larger.

Memory: `project_provekit_libraries_ship_sugar.md`. Future paper 22 (working title: *After Packages: Libraries Ship Their Own Bindings*) makes the architectural argument; the rule of thumb here is the operational consequence.

## Loss behavior when a target has no realization

Not every concept has a binding in every target. A function that lifts cleanly to `concept:http-request` from JavaScript may need to realize into a C program. If the catalog has a libcurl binding, the realize succeeds. If it does not, the substrate has three principled options, none of which is "pretend it worked":

### Refuse

Emit a `lossy-realization-refusal` memento citing the missing cell. The realize halts. The maintainer sees the refusal and either authors the missing cell, accepts a loudly-bounded-lossy stub (see below), or chooses a different target.

Refusal is the default for safety-critical contexts. It is also the default for concepts whose contract is too strong to weaken without changing program behavior in user-visible ways (a cancellation guarantee, a timeout invariant, a transactional boundary).

### Loudly-bounded-lossy stub

Emit a target-surface stub that satisfies the concept's contract on a documented subset of inputs and explicitly refuses or panics on the rest. The realize succeeds, and the emitted code carries an `observed_loss_record` memento that names the bounded loss precisely.

This is the operational form of "we know what we cannot express, and we say so out loud in the emitted code." It is the only legitimate way to ship a partial realization. Anything quieter is wrong under *Supra omnia, rectum*: never claim more than you can prove, and a stub that pretends to be complete claims more than it is.

### Generated candidate

Dispatch a candidate-generation request to the generative-completion pipeline (#841). The pipeline proposes a target-surface realization; the discharge step runs; if it passes, the candidate is accepted and the realize proceeds. If it fails, the discharge becomes a refusal as in path 1.

This is the asynchronous path. The realize step does not block on it; the request becomes a pending memento and the catalog grows lazily.

The three options are exhaustive: refuse, ship a bounded stub with a receipt, or escalate to generative completion. Silently emitting an incorrect target is not in the option set. That is the rule the architecture enforces, not a discipline the maintainer is asked to remember.

## Why this is M+N, not M×N

If there are M source libraries and N target libraries for a given concept, the naive translation count is M×N pairwise translations. The catalog's count is M+N: one lift entry per source library, one realize entry per target library. The hub mediates.

For K concepts, the total catalog work is the sum over k of M_k + N_k, not the product. Adding a new library on one axis adds exactly two entries (lift + realize) per concept it touches; it does not interact combinatorially with the libraries on other axes.

The math is the same math as cross-language transport. The new claim is only that the math holds at the API tier, because the API tier mints the same kind of memento as the primitive tier, and the address space is flat.

## Acceptance for this doc

A reviewer who reads this should be able to answer:

1. Why does the catalog not need a separate "library layer"? Because high-level library calls bind to concept CIDs the same way primitive operations do; the layer is the catalog itself.
2. Why is cross-library transport M+N? Because the hub mediates per concept; libraries do not transport pairwise.
3. What are the four ways a binding gets into the catalog? Authored, Inferred, Generated (#841), Self-Attested.
4. What happens when a target has no realization for a concept the source needs? One of three principled outcomes: refuse, loudly-bounded-lossy stub with receipt, or generative-completion candidate. Never silent emission.
5. What is the first API-tier backlog? HTTP request/response, deep-equality, JSON encode/decode, regex-match, retry-with-backoff, debounce.

The trinity HTTP receipt (issues #847, #848, #849) is the first empirical demonstration of this rule. Paper 21 (`docs/papers/21-after-cross-language-every-cross-x-dissolves.md`) is the argument. This doc is the design clarification that sits between them.
