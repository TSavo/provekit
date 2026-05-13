# ProvekIt: The `.proof` File

Status: Executive summary by T Savo. The formal protocol specification is the [bluepaper](02-bluepaper.md). The closing argument is [paper 14](14-after-trust-the-universal-correctness-bundle.md).

## 1. The Deliverable

A `.proof` file is a constant-size, locally-verifiable correctness guarantee. The substrate is the factory; the `.proof` file is the artifact you ship and the artifact you check. It works for any domain whose claims can be expressed as content-addressed predicates: software, hardware, physics, legal contracts, finance, medical devices, supply chains.

That sentence is the product boundary. The protocol underneath matters because it makes the artifact portable, comparable, signed, content-addressed, and cheap to verify. But what a downstream user receives is not a theory, a pitch deck, a standards body process, or a vendor assurance letter. They receive bytes whose filename is their content identity and whose contents can be checked locally.

A memento is a JCS-canonical JSON object carrying a binding hash, a property hash, an evidence body, and a producer Ed25519 signature. Its CID is BLAKE3-512 of its canonical bytes. A `.proof` file is a deterministic-CBOR catalog wrapping one or more mementos; its filename is its own CID. The Rust peer at `implementations/rust/` and the C++ peer at `implementations/cpp/` produce byte-identical outputs from the same inputs. Conformance is not a slogan; it is a byte property.

The verifier algorithm is deliberately small:

1. Recompute each memento CID from canonical bytes.
2. Compare requested CIDs with the recomputed 64-byte values.
3. Verify producer Ed25519 signatures.
4. Walk only the implication edges required by local policy.
5. Accept, refuse, or ask for a deeper witness.

The lattice is the directed acyclic graph of every memento anyone has published. Edges are `inputCids`. A verification query discharges through three tiers:

- Tier 1: hash equality on a 64-byte digest, one `memcmp`, measured at about 58 ns.
- Tier 2: cached implication memento lookup plus one Ed25519 signature verify, measured at about 66 us.
- Tier 3: witness from scratch, using z3, cvc5, Vampire, a notary, or a lab instrument, measured at about 24 ms.

Those numbers come from `docs/launch/showcase-results.md`, measured against a fixture lattice of about 1.1 million signed mementos occupying about 2.5 GB on disk. The cost of any single query is 64 bytes regardless of lattice size. The lattice-tractability theorem in the [bluepaper](02-bluepaper.md) proves the asymptotic. The benchmark shows the engineering shape: once a claim has a CID, the local question is not "how large is the world below this claim." It is "does this byte string equal that byte string under my policy."

## 2. Why It Matters

Every prior verification approach has a cost that grows with what is verified. Type systems type-check the program. Theorem provers discharge from axioms. Static analyzers traverse code. SBOMs enumerate dependencies. Certification packets get larger as systems get larger. Even when the result is useful, the check is coupled to the size and complexity of the thing being checked.

Content-addressed systems escape that shape. Bitcoin, Git, IPFS, and BitTorrent are the canonical lineage. Bitcoin content-addresses transactions and blocks. Git content-addresses source history. IPFS content-addresses files. BitTorrent content-addresses chunks. Each system replaces an authority bottleneck with local verification of content identity. ProvekIt extends the lineage by one rung: content-addressed trust-free systems for verifiable propositions.

The key is that any sub-DAG is a self-contained trust unit. Stop at any node; the node is your verification anchor. Walk deeper only when your local policy requires it. Trust depth is configuration:

```toml
[verification]
trust_depth = 1            # only my own .proofs (CI default)
# trust_depth = 5          # walk through transitive deps
# trust_depth = "silicon"  # full chain to physics (medical / aerospace)
# trust_depth = "blake3-512:..."  # stop at specific anchor
```

A CI system can stop at its own release catalog. A library author can stop at the catalog they signed. A security auditor can walk five hops to the OS syscall layer. A medical-device certifier can walk to the physical assumptions beneath a sensor. Same protocol, same 64-byte content identity, different stopping depth.

Above the anchor: math. Below the anchor: trust.

That line is not rhetorical. It is the operational split. ProvekIt does not demand that every user verify all the way down. It lets every user state the depth at which trust begins, then makes everything above that anchor locally checkable.

## 3. After Trust

The old trust stack asks "who said this is fine." The answer might be an auditor, regulator, certification body, peer reviewer, vendor, package registry, code-signing PKI, or third-party security firm. In aviation the answer may be DO-178C. In security it may be Common Criteria EAL. In automotive it may be ISO 26262. In medical software it may be FDA SaMD review. In open source it may be maintainer reputation, registry integrity metadata, and a transitive chain of hope.

A `.proof` replaces the epistemic core of that arrangement. It does not replace the legal power, economic role, or human responsibility of institutions. Regulators still regulate. Courts still assign liability. Vendors still make warranties. Auditors still choose scopes. Engineers still write weak contracts sometimes. What changes is the center of the factual claim.

The old statement is: "This is fine because an authority said so."

The new statement is: "The math says this bounded claim checks under this policy; here is how to verify it yourself."

This is after trust in the authority sense. It is not after policy, governance, liability, or judgment. Institutions become policy authors and risk owners rather than the only practical proxy for truth. A regulator can require a portfolio and verify locally. A small manufacturer can ship the same kind of machine-checkable evidence that previously required a certification moat. An air-gapped operator can validate release artifacts without calling home. A maintainer twenty years later can inspect the exact claims that bound a dependency when it shipped.

The participation boundary moves. Today, correctness evidence often requires institutional access: the right lab, the right auditor, the right vendor portal, the right procurement channel. A content-addressed `.proof` file makes bounded correctness evidence transmissible as an artifact. Anyone with the bytes, the policy, and the verifier can check the claim.

## 4. The Bedrock Thesis

The bedrock thesis is literal, not metaphorical. The runtime layer of essentially every deployed software system on Earth is C or C++ source on disk, in version control.

Linux kernel: C. glibc and musl: C. Node.js: C++ under the JavaScript surface. V8: C++. CPython: C. PHP Zend: C. Ruby MRI: C. OpenJDK HotSpot: C++. .NET CoreCLR: C++. Erlang BEAM, GHC RTS, Lua, Perl, and Tcl: C. PostgreSQL, MySQL, Redis, SQLite, nginx, Apache httpd, OpenSSL, libuv, libcurl, ffmpeg, ImageMagick, git, bash, zsh, and tmux: C.

That fact matters because ProvekIt does not need a new civilizational rewrite before the catalog becomes useful. The existing C lifter chain, `collectors-defensive`, `kunit`, `assertions`, `kernel-doc`, `sparse`, and `walk-c`, handles every one of these projects today, unmodified. There is no new class of engineering required: clone the repo, add a target stanza, run the pipeline.

The scale is also tractable. Across roughly twenty major bedrock projects, the total surface is about 80 to 150 million lines of C and C++ source. At measured throughput of 25 to 31 ms per file per lifter on a 32-core machine, the whole bedrock lifts in 4 to 8 hours. That is one overnight ingestion run, not a decade-long rewrite campaign.

This is why the `.proof` story is tractable rather than aspirational. A `.proof` against an unpopulated catalog is a map of holes. It can still say useful things about the top layer, but every unchecked dependency boundary becomes a trust anchor by accident. Once the bedrock catalog is populated, the same application composes downward.

A Node app is the clearest example. Without bedrock proofs, a proof for the app stops at JavaScript, native bindings, V8, libuv, OpenSSL, and the platform boundary. With bedrock proofs, it composes through V8, libuv, OpenSSL, its native bindings, its package graph, and its user code into one root claim small enough to ship in a Docker label. The deliverable stays small because the catalog has already absorbed the lower layers.

Source availability alone does not prove correctness. It makes the bedrock liftable. The difference is decisive.

## 5. The Supply Graph

Every package registry is a content-addressed federated graph in waiting. npm, PyPI, crates.io, RubyGems, Maven Central, NuGet, and Hex already carry package identities, version identities, dependency edges, integrity metadata, install scripts, source links, maintainers, release timestamps, and artifact hashes. They are almost proof graphs already. They lack a common predicate envelope and a verifier that treats those facts as first-class receipts.

A registry lifter mints mementos for those facts. Package version identity becomes a memento. Dependency edges become mementos. Integrity fields become mementos. Install scripts become explicit execution claims. Source links become content-addressed bindings. Release attestations become signed receipts. The graph stops being prose around package management and becomes queryable correctness material.

The native boundary is the hard part today, and it is exactly where the bedrock catalog pays off. Registry lifters can cross-link packages to the native C and C++ source they wrap: `sharp` to libvips, `bcrypt` to native crypto, `node-sass` to libsass, `better-sqlite3` to SQLite, `canvas` to Cairo, and brotli, zlib, snappy, and lz4 wrappers to native compression libraries. Once bedrock is in the catalog, those wrappers' native trust surface is already verifiable.

Postinstall sabotage becomes a detectable graph pattern rather than a registry morality play. Incidents like eslint-scope credential theft, ua-parser-js malware, colors.js self-sabotage, and left-pad descendants are different stories at the social layer, but they rhyme at the graph layer: unexpected install behavior, maintainer or release discontinuity, new artifact bytes, new dependency reachability, and policy refusal receipts. A ProvekIt registry lifter can express that pattern without pretending every package maintainer is a formal methods expert.

CVE blast-radius becomes a `SELECT` over content-addressed facts plus reachability, not a prose exercise over guessed version ranges. Which shipped artifacts contain this vulnerable source CID. Which packages wrap it. Which Docker images include those packages. Which services deployed those images. Which `.proof` files still accept the affected path under policy. That is a database query over signed facts, not a spreadsheet assembled during an incident.

## 6. Who It Is For

ProvekIt has three primary audiences and three CLI paths.

Developers with existing test cultures get the surface plugin layer and `provekit lift`. A `proptest` strategy becomes a forall contract memento. A Zod schema becomes a precondition memento. A `kani` harness becomes an invariant memento. Unit tests, property tests, schemas, harnesses, annotations, and language-specific contract surfaces lift into the same predicate envelope. There is no migration. The developer keeps their test culture; the lift adapters in the Rust workspace translate existing evidence into catalog material.

Library authors and infrastructure publishers get the catalog release flow. The current CLI command is `provekit mint`: it drives the configured lift plugin, emits the proof envelope, and writes the `.proof` artifact. In a release pipeline, that step produces a signed `.proof` file beside the library, image, firmware blob, model, or package. Downstream consumers pull the bundle alongside the artifact. The library's API claims are now machine-checkable. The consumer's call sites can be checked against those claims. The verification path is content identity plus policy, not coordination with the publisher.

Users who would rather state intent in English get the agent layer and `provekit must`:

```sh
provekit must app.ts "users can't have negative balance"
```

The agent reads the file, proposes a contract in the configured authoring surface, validates it against the canonical IR grammar, mints the memento, signs it, and writes the `.proof` material. The backend slot is pluggable by manifest: Claude Code, Codex, OpenCode, OpenAI, a local model, or a domain-specific model can sit behind the same CLI shape. The verifier does not trust the model. It trusts only the memento bytes, signatures, compiler outputs, and local policy.

These paths are intentionally ordinary. `provekit lift` over an existing test corpus. `provekit mint` in release automation. `provekit verify` or `provekit prove` in CI and admission control. The substrate can be general without forcing every user to encounter the whole substrate on day one.

## 7. The Prove Portfolio

No single proving engine owns correctness. ProvekIt uses a portfolio because different theories have different authorities.

The current portfolio is z3, cvc5, Vampire, and Coq. z3 and cvc5 cover the SMT workhorse path. Vampire covers equational reasoning via superposition. Coq covers proof terms, induction, and ring and field tactics. Each compiler is the authority on what its target theory soundly handles. The verifier's authority is composition, not translation.

That distinction is load-bearing. ProvekIt does not claim that any English sentence, annotation, schema, or legal clause magically becomes true after translation. A compiler emits a bounded predicate into a theory. A solver or proof assistant produces evidence under that theory. The memento records the binding, property, evidence, producer, and signature. Composition records how one bounded claim implies another. The verifier checks the receipts under policy.

The portfolio is extensible. A Maude equational backend with a CeTA-certified termination and confluence gate is in flight. A Lean 4 plus mathlib backend is in flight. Those additions grow the range of claims that can be discharged soundly; they do not change the `.proof` envelope or the local verification model.

## 8. What It Is Not

ProvekIt is not a better static analyzer. Static analyzers find facts and warnings inside code. ProvekIt transports bounded correctness claims as signed, content-addressed artifacts and lets other tools contribute evidence.

It is not a package manager with proofs attached. Package managers resolve and install artifacts. ProvekIt can lift package registries into proof graphs, but the `.proof` file is the common envelope, not a registry replacement.

It is not a blockchain. There is no consensus protocol in the verifier path, no token, no global ordering requirement, and no need to ask the network whether a local claim checks. [Paper 3](03-substrate-not-blockchain.md) gives the consensus-free validity argument and the pin-as-rank-N-tuple discipline.

It does not make a false predicate true, a weak contract strong, or an unsound solver sound. If a lifter emits the wrong legal predicate, the resulting legal claim is wrong. If a medical-device lifter misstates the relevant safety property, the `.proof` file faithfully transports a bad claim. If a solver backend is unsound for a theory, policy must reject it or constrain it. Domain lifters and prove portfolios must be domain-correct.

Source availability alone does not prove correctness. A public repository is not evidence by itself. It is input material that can be lifted, bound, checked, and signed.

Institutions do not vanish. They set policy, decide acceptable portfolios, define certification scopes, assign liability, and decide what evidence is sufficient for a domain. What they stop being is the only practical proxy for checking the artifact's bounded truth.

## 9. The Ladder

This paper is the pitch: the shortest path from hearing the name ProvekIt to understanding the move. [Paper 2](02-bluepaper.md), the bluepaper, is the formal specification: canonicalization, memento shape, proof-file format, verifier semantics, lattice tractability, and the executable verification discipline. Papers 3 through 14 are the After X ladder: each rung says, given what the prior paper established, here is what changes.

New readers should especially read three rungs.

[Paper 2](02-bluepaper.md) defines the protocol. If a claim in this paper sounds too convenient, the bluepaper is where the bytes, grammars, and proofs are pinned.

[Paper 3](03-substrate-not-blockchain.md) explains why this is substrate, not blockchain. Its core point is consensus-free validity: local content identity plus pinned policy is enough for the check ProvekIt needs. Its pin-as-rank-N-tuple discipline names how protocol, portfolio, policy, and artifact identities remain explicit instead of collapsing into social trust.

[Paper 14](14-after-trust-the-universal-correctness-bundle.md) is the closing argument. It names the universal correctness bundle and states the six load-bearing lemmas: bedrock dominance, Bridge minimality, plateau finiteness, domain agnosticism, constant-size verification preserved under composition, and CVE blast-radius is SELECT.

The README index does the full table of contents. The practical install path today is build-from-source:

```sh
cargo install --path implementations/rust/provekit-cli
provekit init
provekit lift
provekit mint
provekit verify
```

The intended public package shape is the same CLI surface. The repo path is current because this is a live worktree, not a marketing artifact detached from the implementation.

## 10. The Trojan Horse

ProvekIt ships as a normal developer tool: a CI step, a `.proof` file beside an artifact, a `provekit lift` over an existing test corpus, a Docker label, an admission check, a release receipt. That is the adoption path because it matches how software already moves.

The structure underneath is the cypherpunk endgame: verification without authority, local check, trust replaced by math. Adoption does not require anyone to believe the endgame. It requires `provekit lift` to be cheap and `provekit verify` to be a `memcmp`. The catalog grows by use. Every lifted test, schema, harness, package edge, native wrapper, release artifact, and bedrock source claim adds another signed fact to the lattice. The next verifier does not pay again for the whole world. It checks the bytes it needs under the policy it chose.

This is why the deliverable must stay central. The substrate can become large, multilingual, multi-domain, and multi-institutional. The `.proof` file remains the artifact you ship and the artifact someone else can check. It is the common envelope for claims that used to live in incompatible systems: test reports, solver logs, audit packets, certification evidence, registry metadata, lab output, legal attestations, and release signatures.

Install it. Lift your code. Ship the `.proof`. Verify other people's `.proof` files. Refuse artifacts whose receipts do not satisfy your policy. Publish refusal receipts so the graph learns from the boundary.

The after-trust question is not "who says this is fine." It is: what is claimed, what receipts bind it, what policy accepts it, can I verify it here.

The answer is the `.proof` file.
