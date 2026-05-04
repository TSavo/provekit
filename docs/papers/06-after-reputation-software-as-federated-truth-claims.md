# After Reputation: Software as Federated Truth-Claims

> **Status.** Sustained argument. Engages counterarguments. Written to be cite-able.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [02 Bluepaper](02-bluepaper.md), [03 Substrate, not Blockchain](03-substrate-not-blockchain.md), [04 Vertical Stack and Standardization](04-vertical-stack-and-standardization.md), [05 Witness Pluralism and Jurisdiction-Neutral Transport](05-witness-pluralism-and-jurisdiction-neutral-transport.md).
>
> **Premise the earlier papers established.** A protocol for content-addressable, cryptographically-signed, byte-deterministic claims about software behavior, federated across signers, composable end-to-end, jurisdiction-neutral, and machine-checkable. Claims about claims about software, written down once, verified anywhere, federated by anyone with standing.
>
> **What this paper argues.** That the consequence of shipping that protocol is not a better security tool. It is the substrate replacing reputation as the load-bearing trust mechanism in software supply chains, software engineering practice, software liability, and the relationship between closed and open source. The substrate is the diplomatic protocol between every truth-claim about software ever made.

## §0 — The claim

Today, the artifact of software is code. Tomorrow, the artifact of software is the proof. Code becomes one implementation of the proof. Refactoring becomes generating different implementations of the same proof. AI becomes a proof-implementation generator. The engineer's job shifts from *write code that works* to *write the contract; agents write the code*.

This paper argues that the substrate ProvekIt ships is the move that makes that shift not aspirational but mechanical. That shift cascades through software's value chain: engineering practice changes, supply chains become computable, type systems get demoted to syntactic conveniences, insurance finds a footing it has been waiting decades for, and the open-vs-closed source dichotomy collapses into a question of which signer's keys you pin.

We are after reputation. The substrate makes trust composable. What follows is what falls out.

## §1 — Today's substrate

Software in 2026 runs on a substrate of reputation, brand, and habit.

A developer types `npm install lodash` and gets ~600KB of JavaScript signed by no one in particular, fetched from a registry whose authority derives from being the place everyone fetches from. The lodash package's identity is its name. Its trustworthiness is the brand. Its claims about itself are the README. Its actual behavior is whatever the JavaScript happens to do at runtime, which may match the README or may not, and there is no mechanism to know which without reading every line.

This works as well as it does because of social engineering, not because of any property of the substrate. Maintainers maintain because their reputations depend on it. Reviewers review because they care. Users update because the alternative is exposure to known vulnerabilities. The whole system holds together because enough humans, often enough, do the right thing.

When humans don't do the right thing, the substrate is silent.

- The `event-stream` attacker added a malicious dependency to a popular package. The substrate said nothing; the package's name was unchanged, the registry served it, the install command succeeded. Detection eventually came from a developer noticing odd behavior. The substrate offered no help.
- `colors.js` and `faker.js` were sabotaged by their own author. The packages still installed, signed by the same author, served by the same registry. The substrate could not distinguish "this is the package you wanted" from "this is a different package by the same author with the same name."
- The Solarwinds compromise propagated through a build system whose attestations were trusted because the build system was trusted. The substrate had no mechanism to ask "do these claims about behavior compose?" because there were no claims about behavior to compose.

The pattern is consistent. Reputation can be inherited (a maintainer hands off a package), bought (npm typo-squatting), borrowed (compromised credentials), or simply lost (the maintainer goes off the rails). When reputation is the substrate, every failure of reputation becomes a failure of software, and the substrate is silent because there is no mechanism in it for reasoning about behavior.

The substrate of reputation is also the substrate of the gap between what software claims and what software does. There is no formalism for the claim. The README is text; the behavior is bytes; nothing connects them. Documentation drifts from code because there is no machinery enforcing the connection. Specifications drift from implementations because specifications are documents and implementations are runtime.

This is what we are leaving behind.

## §2 — The shift

Replace reputation with proof. Replace the package's identity (its name) with its content-CID. Replace the package's claims about itself (the README) with signed contracts about its behavior. Replace the substrate's silence with a verifier that answers, for any composition of dependencies, whether the composed behavior is consistent with the composed claims.

Three pieces have to be in place for this to work. None of them are individually new; what is new is that all three are simultaneously cheap enough to run on every save.

**Cryptographic content-addressing.** Every artifact (function, module, binary, document, contract) is named by the BLAKE3-512 hash of its canonical bytes. There is no naming authority and no way to spoof identity by renaming. `lodash@4.17.21` is identified by `blake3-512:9f3a...`; a sabotaged copy of "the same thing" is identified by a different CID, and any consumer who pinned the CID rejects it on sight.

**Signed claims.** Every contract about behavior — preconditions, postconditions, invariants, type signatures, length bounds, side-effect properties — is wrapped in a signed envelope. The signer's identity is content-addressed (an Ed25519 public key). The contract's identity is content-addressed (BLAKE3-512 of the canonical encoding of the claim). Forgery requires breaking Ed25519 or BLAKE3, both of which are out of reach for the foreseeable future.

**Composable verification.** Given a composition of artifacts (your application linking against lodash linking against its dependencies), the verifier composes the contracts: your application's properties are derivable from the composition of every dependency's signed contracts. If the composition is consistent, the application is verified. If it is inconsistent — your code's preconditions for some callsite are not established by the upstream's postconditions — the verifier surfaces the gap, with a proof witness showing why.

The shift is from substrate-of-reputation to substrate-of-claims. Trust is no longer "who do you know" but "what is signed, by whom, that composes to what your application requires." Reputation does not disappear from the picture — you still pin keys belonging to entities you trust — but reputation moves from the substrate to the policy layer. The substrate's job becomes mechanical: verify claims, compose claims, surface mismatches. Trust policy is the consumer's call.

## §3 — Engineering practice consequences

When proofs are first-class artifacts, several practices that defined software engineering for forty years stop being load-bearing.

**The end of "works on my machine."** A claim that holds on one machine and not another, given the same inputs, is not a claim. With byte-deterministic verification, a proof that verifies on my machine verifies on every machine running the substrate. If your machine produces a different verdict, your machine is the bug. The conversation shifts from "why does this fail in production" to "what configurational drift in production violates a precondition that held in test." The class of failure that resists reproduction shrinks dramatically.

**The test pyramid demoted.** Tests are sampled assertions: I checked these inputs, the behavior held. Proofs are universally quantified: for all inputs satisfying the precondition, the postcondition holds. The two are not in opposition. Tests validate that contracts are correctly stated; the contracts then carry the universal claim. But tests stop being the primary quality measure. The primary quality measure becomes contract coverage: how much of the program's behavior is captured in signed contracts. A program with 100% test coverage and 10% contract coverage is fragile. A program with 60% test coverage and 95% contract coverage is robust against inputs nobody tested.

**Type systems become a syntactic convenience.** Type systems are weak proofs about narrow shapes. "This function takes an Int" is one conjunct of a precondition that might also include "and the Int is non-negative, and less than the array length." Once the substrate carries the full precondition, the type system's job is to make the syntax of the precondition tractable. Languages without type systems (PHP, Python pre-Pyright, Ruby) catch up to languages with them, because the substrate doesn't care which language the claim is bolted to. The compiler's role is to extract claims from source; the source's syntax is incidental.

**Code review becomes contract review.** A reviewer today asks: does this code do the right thing? A reviewer with the substrate asks: does this contract capture the right thing? Implementation correctness against a contract is mechanically verifiable; contract correctness against intent is the only thing humans need to evaluate. This is a substantial reduction in the surface area of human review. It also re-centers the activity: code review becomes design review.

**Refactoring becomes bounded.** Any code transformation that preserves the contract is automatically safe; any transformation that breaks the contract is automatically caught. AI-driven refactoring becomes safe at scale because the contract catches drift. The category of "I don't want to touch this code because nobody understands it anymore" shrinks: the contract IS the understanding, the code is one implementation.

**Bugs become contract violations with witnesses.** A bug today is a vague indictment of code: "this doesn't work right." A bug under the substrate is a precise indictment with a witness: "for input I, which satisfies precondition P, postcondition Q is violated." The CVE process changes shape. Vendors no longer need to triage "is this really a bug" — the witness is unambiguous. Patches become proof restorations rather than guesses; the patched code's contract is verified before the patch ships.

These changes are not speculation. They are mechanical consequences of the substrate. Each can be derived directly from the protocol's primitives.

## §4 — Supply chain consequences

This section is where the change is largest and least appreciated.

Software supply chains today are reputation propagation. You install a package, transitively pulling in dozens of dependencies. Each dependency's trustworthiness derives from its maintainer's reputation, the registry's curation, and habit. When the chain breaks — a maintainer is compromised, a registry is exploited, a build system is suborned — the substrate offers no mechanism for detecting that the failure has occurred until something visible breaks.

Replace the chain with composition.

**Dependency confusion attacks become arithmetically impossible.** When `lodash` and `lodahs` are both names in a registry, an attacker can publish a typo-squatted package and a fraction of users will install it. With CIDs as identity, `lodash@4.17.21` is `blake3-512:9f3a...`; a typo-squatted package has a different CID; a consumer who pinned the legitimate CID rejects the typo-squat on sight. Renaming attacks evaporate. Brand-equity attacks evaporate. The class of attack that depends on humans confusing names is closed.

**Software bills of materials become meaningful.** An SBOM today lists names: "this binary contains version X of library Y." This is theatre — the SBOM offers no mechanism to verify behavior against. An SBOM under the substrate lists CIDs and signed contracts: "this binary's behavior is the composition of these signed contracts; here is the proof; here is the chain of signatures." The downstream auditor doesn't have to take anyone's word for anything. They run the verifier.

**Transitive trust becomes a Merkle composition.** Today, if you install foo which depends on bar which depends on baz, you are implicitly trusting three signers, three release processes, and three review chains. Each transitively-trusted entity is a hidden assumption. With signed contracts, the composition is explicit: foo's claims about its behavior depend on bar's claims about its behavior depend on baz's claims about its behavior. Each link is a signed claim from a key you pin. If you don't pin a key in the chain, the chain breaks at that point and surfaces. Hidden assumptions become visible.

**Patch fatigue ends.** Today every dependency update is a risk event: maybe this update breaks things; maybe it introduces a vulnerability; maybe it changes behavior in subtle ways. Update fatigue accumulates because the substrate offers no mechanism for telling safe updates from unsafe. With contracts: an update either preserves the contract (safe by construction; the verifier confirms) or breaks it (caught at install before any code runs). The decision to update reduces to "do I want the new contract." Most updates won't change the contract; most updates therefore become trivially safe.

**License compliance gets honest.** GPL says: include this license if you link to GPL code. The phrase "linking" is a forty-year gray zone because nothing in the substrate makes "linking" mechanical. With contracts: the verifier knows which functions are reachable from your binary. License obligations follow actual reach, not lawyer hand-waving. This is good news for honest actors and bad news for actors who have been getting away with strategic ambiguity.

**Insurance and liability find footing.** This deserves its own paragraph because it is the most underweighted consequence of the shift. Software liability today is "we promise nothing; see EULA." This is not because no one wants liability; it is because the substrate offers no mechanism for software to make claims an underwriter can underwrite. A medical device manufacturer cannot insure "this device behaves correctly under conformant input" because there is no formal definition of "behaves correctly" the insurer can audit. With signed contracts, the manufacturer's claims become formal; the insurer can audit them; the device's failure to satisfy a claim becomes a verifiable event that triggers the policy. The insurance industry has been waiting decades for software to make claims it can underwrite. The substrate is the missing piece.

This last point cascades. Once any class of software can be insured against contract violation, every other class of software is comparatively underinsurable. Capital flows toward the insurable. The pressure to ship signed contracts is no longer "best practice"; it is "you cannot get insurance otherwise."

## §5 — The closed-vs-open source reckoning

Open source has long held an implicit advantage in trustworthiness: you can read the source, therefore you can audit the behavior. The fine print: almost nobody actually reads the source of the libraries they depend on. The "open source means audited" claim is mostly a ritual rather than a practice. But it has been a real advantage in marketing terms, and in regulated environments it has been a real basis for procurement decisions.

The substrate erodes this advantage.

A vendor shipping closed-source software with signed contracts ships a verifiable claim about behavior. The downstream consumer does not need to audit the source to verify the claim; they verify the proof. "Trust us" becomes "verify us." The closed-source vendor's signature carries the same weight as the open-source maintainer's signature, evaluated by the same protocol, audited by the same machinery. The auditor's job is identical in either case.

This is uncomfortable for parts of the open-source movement that have come to depend on the open-source-means-trustworthy framing. It should be reckoned with honestly. Open source still has many advantages: extensibility, forkability, freedom-as-in-speech, community. But "harder to audit because closed-source is opaque" is not a real advantage when the alternative is a verifiable proof.

The countervailing force: open source can ship the same proofs, and additionally the source. The combination dominates closed-source-with-proofs. So the move forward for open source is not to reject the substrate but to embrace it harder than closed source can. The substrate is a powerful ally for open source if open source treats it as one.

For closed source, the substrate is a license to compete on equal trustworthiness terms. This is good for buyers in regulated industries, bad for vendors who have been hiding behind closed source as a proxy for "you can't audit our claims." Vendors who never made claims worth auditing now have to make them.

## §6 — Why now

The pieces have been around a long time.

- Hoare logic dates to 1969.
- Formal methods were a research field by the 1970s.
- Content-addressable storage was a research idea in the 1980s.
- Cryptographic signatures became practical in the 1990s.
- Package managers proliferated in the 2000s.
- Distributed verification — Bitcoin, Git, IPFS — became default cultural mental models in the 2010s.

The IDEA of "every piece of software ships with a signed proof of its behavior" is not new. The pieces have been shippable, separately, for a generation. Yet here we are in 2026 and software still runs on reputation. Why?

Three constraints relaxed roughly simultaneously, and the substrate finally became cheap enough to run on every save.

**Hash functions became fast enough.** SHA-256 was acceptable for occasional use; not for hashing every save in an editor in real time. BLAKE3 (2020) is fast enough that hashing on every keystroke is invisible. Verification on every install is invisible. The cost-of-substrate floor dropped below the threshold at which it becomes invisible to the user.

**AI became capable enough to author proof material at scale.** Hand-authoring a baseline catalog for a language with 5000 standard-library functions is a multi-decade project for human engineers. The same project is days for a sufficiently capable language model directed by good prompts. The bottleneck on substrate adoption used to be "who writes the contracts." That bottleneck dissolved when AI got good enough to do the writing.

**Distributed signature verification became cheap.** Ed25519 (2011) on commodity hardware verifies in microseconds. PGP-of-yore made signature checking feel expensive and ritualistic; the modern stack makes it free. Every save can sign; every load can verify. The cost is invisible.

These three relaxations are recent — within the last decade for hashing and signing, within the last few years for AI authoring. The substrate is finally cheap enough that "every save signs, every load verifies, every save indexes claims, every diff propagates verification" stops being a performance issue. Combined with the cultural shift to content-addressing as default mental model (post-Bitcoin, post-Git, post-IPFS), the substrate stops being a research curiosity and starts being a normal way to build software.

The window is now. Five years ago, the cost of the substrate was visible to users; the substrate was research. Five years from now, every major language will have its own ad-hoc version of this and they will not interoperate. The window for a single content-addressable substrate that federates across every language is now.

## §7 — Counterarguments

This section engages the obvious objections seriously. Each is real; each has a response; some responses are partial.

### "Formal verification has been tried; it doesn't scale."

Formal verification with hand-authored full functional correctness proofs (CompCert, seL4) is research-grade and does not scale. The substrate this paper describes is not that. It is signed contracts at whatever density the signer can afford, composed across signers, verified locally. A signer can sign weak contracts (just type signatures and length bounds) and the substrate still works; verification finds the gaps. The substrate is therefore not in competition with full functional correctness — it is the floor on which full functional correctness is one extreme and signed-type-signatures-only is the other. Both are valid. The substrate makes the spectrum useful.

### "Nobody will write the contracts."

This was true before AI authored proof material at scale. It is no longer true. Contract authoring for any given language's standard library is now days of automated work, supervised by humans, signed by humans. The bottleneck moved from "humans hand-authoring 5000 contracts" to "humans curating prompts and signing 5000 contracts." The latter is tractable.

### "The contracts will be wrong."

Some will be. The substrate's response: contracts are content-addressed, signed, and pinable. A buggy contract has a CID; a corrected contract has a different CID; consumers can pin either or migrate. The substrate makes contract correctness an iterative engineering activity rather than a one-shot bet. This is the same response a software ecosystem gives to "the code will be wrong": yes, and we have versioning.

### "This requires every language to participate."

It requires every language one cares about to participate, which is not the same thing. The substrate is content-addressable; it does not care which language the contract is about. A C# contract about a C# function is byte-equivalent to a C# contract about the same C# function on a different machine. A consumer pinning a C# contract does not need every other language to participate.

What the substrate enables is per-language participation. The first language whose stewards sign canonical contracts about its standard library makes that language demonstrably more trustable than the others. The pressure to follow is real. Languages whose stewards refuse to sign are languages whose users have to use foundation-baseline-advisory contracts, which is a fine fallback but visibly inferior. The substrate creates pressure for stewards to sign without forcing them to.

### "What about legacy code that doesn't have contracts?"

Two responses. First: contracts can be added incrementally. A function with no signed contract is a function with `top` precondition and `top` postcondition — anything in, nothing claimed about out. Code that calls such functions falls back to its own contract (which may include manual assertions). The substrate degrades gracefully; it does not require all-or-nothing adoption.

Second: AI-driven contract inference can populate contracts for legacy code at scale. The contracts won't be optimal but they will be better than nothing. The signer's role is to audit and bless the inferred contracts. The substrate is a force multiplier for legacy modernization, not an obstacle to it.

### "Cryptographic substrates have been promised before."

True. Bitcoin promised peer-to-peer cash; what shipped was speculative finance. IPFS promised the new web; what shipped was an interesting peer-to-peer layer with niche adoption. Crypto promised programmable money; what shipped was casinos. The track record of cryptographic-substrate-promises is not encouraging.

The response is twofold. First: the substrate this paper describes is not asking for cultural transformation; it is asking for tooling that fits inside existing engineering practice. Developers do not need to change how they think about money or governance or trust; they need to opt into a verification step that runs invisibly. The adoption ask is much smaller. Second: the substrate solves a problem developers actually have (supply-chain trust, dependency confusion, fragile updates) rather than a problem cryptographers wish developers had. The match between the substrate and the demand is tighter.

It may still fail. Predicting cultural adoption is hard. But the failure modes of "developers don't want this" are different from the failure modes of "the technology can't ship." The latter is solved.

## §8 — The diplomatic substrate framing

A remark to close on.

ProvekIt is not a security tool, a verification tool, a type system, a proof assistant, a package manager, or a compiler. It is the diplomatic protocol between all of those. Today each tool has its own model of what's true; they don't compose. Static analyzers produce findings nobody can sign or compose. SMT solvers produce verdicts nobody can pin. Proof assistants produce certificates nobody else's tools consume. Package managers ship code nobody can verify against contracts. Type systems catch a narrow band of errors and stay silent on the rest.

Each of these tools is correct within its scope and silent outside it. They have been silent to each other for fifty years.

Content-addressable signed contracts are the lingua franca that makes them speak. Static analyzers become signers (here is what I checked, here is the contract, signed). SMT solvers become verifiers (this implication holds, here is the certificate, signed). Proof assistants become signers (here is a proof of a hard claim, signed). Package managers become contract distributors. Type systems become contract emitters. AI becomes a contract-implementation generator.

The protocol is not a tool. It is the diplomatic substrate that lets every tool that ever made a claim about software finally talk to every other one.

This is why "every piece of software ships with a real strong proof" is not aspirational rhetoric. It is the operational consequence of the substrate.

## §9 — What this paper is NOT

- It is not a roadmap. The substrate ships at v1.0.0 with foundation baselines as a starting point; everything else is post-launch growth.
- It is not a sales pitch. The substrate is the substrate; whether anyone adopts it is a separate question.
- It is not formally precise. Each claim above could be sharpened with reference to specific protocol artifacts (and several of them are sharpened in earlier papers in this series). The argument is sustained, not airtight.
- It is not exhaustive. The supply-chain section in particular could be a paper of its own; the insurance footing alone is decades of work for that industry.

It is an argument that the protocol's consequences are bigger than the protocol's authors usually claim, and that the consequences are mechanically derivable rather than speculative. Each of the §3 and §4 consequences follows from the protocol's primitives. The civilizational scale is a property of the substrate, not a marketing claim about it.

## §10 — Acknowledgments

The cypherpunks-mailing-list lineage is the formative context for this argument. PGP, Hashcash, b-money, RPOW, smart contracts, hash-as-trust-anchor — these are the conceptual ancestors of the substrate this paper describes. The 1995 dedup-via-hashing insight (Xdrive, predating rsync) is the original architectural cut. Digital Confetti (1998) is the original incentive-aligned-distribution sibling. eDonkey, ShareReactor, BitTorrent, and the entire P2P scene that followed are operational ancestors of the substrate; their architectures composed because content-addressing made composition possible. The substrate is the language-agnostic generalization of architectural moves that have been live in a narrower form for thirty years.

The Apache JCS team page lists the architect of this protocol as a member during the iFilm era. The protocol's lineage is verifiable.

## §11 — Citation

> Savo, T. (2026). *After Reputation: Software as Federated Truth-Claims*. ProvekIt Papers, vol. 6. Content-addressed at: blake3-512:&lt;CID at publication&gt;. Available at https://github.com/TSavo/provekit/blob/main/docs/papers/06-after-reputation-software-as-federated-truth-claims.md.

---

*Last edit: 2026-05-04. Previous papers: [01](01-whitepaper.md), [02](02-bluepaper.md), [03](03-substrate-not-blockchain.md), [04](04-vertical-stack-and-standardization.md), [05](05-witness-pluralism-and-jurisdiction-neutral-transport.md).*
