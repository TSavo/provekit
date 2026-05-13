# After Protocol Specs: How Protocols Actually Evolve

> **Status.** Draft whitepaper. Sustained argument. Contains a theorem. Written to be cite-able after review.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [02 Bluepaper](02-bluepaper.md), [03 Substrate, not Blockchain](03-substrate-not-blockchain.md), [04 Vertical Stack and Standardization](04-vertical-stack-and-standardization.md), [05 Witness Pluralism and Jurisdiction-Neutral Transport](05-witness-pluralism-and-jurisdiction-neutral-transport.md), [06 After Reputation](06-after-reputation-software-as-federated-truth-claims.md), [07 After Verification](07-after-verification-bug-classes-as-missing-edges.md), [08 After Types](08-after-types-stop-logging-trust-the-invariant-solver.md), [09 Lossy Boundary Compression](09-lossy-boundary-compression.md).
>
> **Protocol companions.** `protocol/specs/2026-05-06-extension-protocols.md`, `protocol/specs/2026-05-06-truth-discharge-protocol.md`, `protocol/specs/2026-05-06-grammar-conformance-protocol.md`, `protocol/specs/2026-05-06-checker-bytecode-protocol.md`, and `protocol/specs/2026-05-06-obligation-realizer-protocol.md`.
>
> **Premise the earlier papers established.** A protocol for content-addressable, cryptographically-signed, byte-deterministic claims about software behavior, federated across signers, composable end-to-end, jurisdiction-neutral, and machine-checkable. Paper 09 showed why ProofIR is universal precisely because it is lossy over a narrow domain: contract boundaries.
>
> **What this paper argues.** That the same move applies one layer up. Protocols themselves have contract boundaries. A protocol version need not remain prose plus social trust. It can become a signed, content-addressed, grammar-constrained, invariant-constrained, witnessable artifact. Its adoption can be proven, compared, bridged, pinned, refused, deprecated, and superseded as data.

## Section 0: The claim

The modern internet runs on prose.

HTTP is prose. TCP is prose. TLS is prose. DNS is prose. OAuth is prose. OpenAPI is half prose and half schema. Kubernetes is prose plus YAML schemas plus controller behavior. POSIX is prose plus decades of implementation folklore. The RFC series is one of civilization's most successful coordination mechanisms, but its primary unit of protocol reality is still the written specification plus a community of implementers who read it, argue about it, test against each other, and gradually converge.

That model works. It built the internet.

It also leaks.

The leak is not that prose is bad. Prose is how humans understand intent. The leak is that prose is not the same thing as machine-checkable protocol state. A protocol changes; humans read the new text; implementers update code; test suites adapt; compatibility profiles emerge; old behavior remains in the field; middleboxes depend on bugs; clients pin implicit assumptions; security advisories reinterpret old clauses; libraries expose "strict" and "legacy" modes; conformance becomes a social fact spread across documents, tests, implementations, mailing lists, issue trackers, and folklore.

Protocol evolution today is real, but it is not first-class data.

ProvekIt makes it first-class data.

A protocol version can be represented as a content-addressed graph:

```
protocolSpecCid
grammarCid
stateMachineCid
invariantSetCid
parserCid
checkerCid
policyCid
testCorpusCid
interopCorpusCid
knownAmbiguityCid
  -> protocolProfileRootCid
```

An implementation can then publish:

```
implementationCid
protocolProfileRootCid
adapterCid
testWitnessCid
traceWitnessCid
invariantWitnessCid
refusalCid
  -> implementationConformanceWitnessCid
```

A migration can publish:

```
oldProtocolRootCid
newProtocolRootCid
migrationGrammarCid
compatibilityInvariantCid
bridgeCheckerCid
policyCid
  -> migrationWitnessCid
```

The phrase "upgrade to the new protocol" stops being a request to move trust by social instruction. It becomes a request to adopt a new root, whose relationship to the old root can itself be witnessed.

That is the claim:

**Protocol evolution becomes data.**

Not a migration hidden in docs. Not "upgrade your verifier and trust us." A new protocol version is a signed, content-addressed, grammar-constrained, invariant-constrained, witnessable artifact. Its adoption can itself be proven, compared, bridged, pinned, refused, or superseded.

This paper is about what changes when that becomes normal.

## Section 1: Protocols have contract boundaries

Paper 09 argued that ProofIR can be deliberately lossy because the domain is not "all implementation semantics." The domain is contract boundaries.

Protocols are made of contract boundaries.

An HTTP message boundary says:

```
method token has this grammar
header field has this grammar
Content-Length and Transfer-Encoding interact this way
cache validators mean this
proxy forwarding preserves or rewrites these fields
```

A TCP segment boundary says:

```
sequence numbers advance this way
SYN creates this state
ACK acknowledges this range
FIN enters this teardown path
RST has this effect under these states
window size constrains send behavior
```

A protocol is not merely a document. It is a collection of boundary obligations:

- syntactic obligations;
- semantic obligations;
- state-machine obligations;
- temporal obligations;
- compatibility obligations;
- security obligations;
- extension-point obligations;
- downgrade/refusal obligations;
- migration obligations.

Those obligations are exactly the kind of object ProvekIt can name.

The protocol's prose remains essential. Humans still need rationale, examples, intent, warnings, and explanatory structure. But the prose is not the whole artifact. The machine-checkable boundary of the protocol can be lifted into CIDs, grammars, invariants, witnesses, and policy.

That gives a protocol two faces:

```
human face: prose specification
substrate face: signed graph of boundary obligations
```

The human face explains. The substrate face composes.

## Section 2: The theorem

**Theorem (Protocol Evolution as Data).** Let `P_n` and `P_m` be protocol-version artifacts represented as signed, content-addressed graphs. Let each protocol version name its body grammars, state-machine artifacts, invariant sets, accepted parsers/checkers, policy artifacts, and conformance witnesses by CID. Let `R(n,m)` be a migration, compatibility, or supersession relation between `P_n` and `P_m`, also represented as a signed, content-addressed body-claim with a witnessed root. If core verification is limited to finite byte/CID/signature/reference validation, and if all semantic acceptance is expressed as extension witnesses under explicit policy, then protocol evolution can be represented, verified, compared, adopted, refused, and superseded as data without requiring a core protocol change.

Equivalently:

```
protocol_version = witnessed_root(protocol boundary graph)
protocol_change = witnessed_edge(old_root -> new_root)
adoption = local_policy_accepts(witnessed_edge)
```

The theorem is not that every protocol is now correct. The theorem is that protocol evolution has a machine-checkable representation whose trust boundaries are explicit.

**Proof sketch.** Each protocol-version artifact is a signed byte graph. Core verification checks canonical bytes, CIDs, signatures, references, and core header validity. Grammars, parsers, state machines, invariant checkers, and migration witnesses are extension artifacts. They may be evaluated by extension-aware tooling under policy, but core verification does not execute them. If a grammar conformance check, state-machine check, or migration check terminates and emits a signed/content-addressed witness, that witness becomes another node in the graph. If it refuses or does not terminate, no positive witness exists. A consumer adopts a protocol root by policy: it checks the roots and witnesses it relies on, and refuses the rest. Since every version and relationship is named by CID, protocol evolution is an append-only graph of signed artifacts and witnessed edges. QED.

The theorem's force is not mathematical difficulty. It is architectural placement.

Protocol evolution stops being hidden in the gap between prose and implementation. It becomes a DAG.

## Section 3: From protocol specs to protocol artifacts

A traditional protocol specification is a document. It may contain ABNF, diagrams, state machines, examples, test vectors, and normative words: MUST, SHOULD, MAY, MUST NOT. Implementers read it and build software.

A ProvekIt protocol artifact is a graph.

For a protocol profile `P`, the graph may include:

```
specCid
profileCid
messageGrammarCid
stateMachineCid
invariantSetCid
extensionPointGrammarCid
testVectorCorpusCid
interopCorpusCid
ambiguityRegisterCid
securityInvariantCid
parserCid
checkerCid
policyCid
  -> protocolProfileRootCid
```

The root is not "the protocol" in the metaphysical sense. It is a pinned protocol profile: a particular, signed, content-addressed claim about syntax, semantics, invariants, compatibility, and policy.

This matters because real protocols are rarely single objects.

HTTP is not one thing. It is:

- semantic model;
- HTTP/1.1 message syntax;
- HTTP/2 frame layer;
- HTTP/3 over QUIC;
- caching;
- content negotiation;
- authentication hooks;
- proxy behavior;
- header normalization;
- trailers;
- upgrade;
- CORS;
- request smuggling edge cases;
- status-code registries;
- extension headers;
- security profiles.

TCP is not one thing. It is:

- segment syntax;
- state transitions;
- sequence-number arithmetic;
- retransmission behavior;
- flow control;
- congestion-control profile;
- option negotiation;
- teardown behavior;
- TIME_WAIT;
- reset handling;
- implementation-specific timer policy;
- trace-level conformance under network conditions.

The old move is to say "HTTP" or "TCP" and rely on context to infer which slice of reality is meant.

The substrate move is to name the slice.

```
httpStrictHeaderProfileCid
http1ChunkedDecodingInvariantCid
httpProxySmugglingRefusalCid
tcpRenoCongestionProfileCid
tcpSynAckStateMachineCid
tcpTraceConformancePolicyCid
```

Names become CIDs. CIDs become edges. Edges become policy decisions.

## Section 4: `.proof` for HTTP

A `.proof` for HTTP is not a proof that "HTTP is correct."

That sentence is too large to be useful.

A `.proof` for HTTP is a transportable, content-addressed bundle that says:

```
this pinned HTTP profile contains these syntax grammars,
these semantic obligations,
these state-machine obligations,
these security invariants,
these ambiguity declarations,
these conformance witnesses,
and these refusals
```

Example:

```
httpSpecCid
httpSemanticsProfileCid
http1MessageGrammarCid
httpHeaderFieldGrammarCid
httpChunkedDecoderStateMachineCid
httpContentLengthTransferEncodingInvariantCid
httpProxyForwardingInvariantCid
httpRequestSmugglingRefusalProfileCid
httpParserCid
httpInvariantCheckerCid
httpPolicyCid
  -> httpProfileRootCid
```

An implementation then publishes:

```
serverImplementationCid
httpProfileRootCid
implementationAdapterCid
parserConformanceWitnessCid
chunkedDecodingWitnessCid
headerCanonicalizationWitnessCid
proxyBoundaryWitnessCid
refusalCid
  -> serverHttpConformanceWitnessCid
```

A client library can depend on:

```
serverHttpConformanceWitnessCid
```

or, more narrowly:

```
headerCanonicalizationWitnessCid
chunkedDecodingWitnessCid
proxyBoundaryWitnessCid
```

depending on which property it needs.

The practical effect is sharp. A security-sensitive client does not need to ask "does this server support HTTP/1.1?" The useful question is:

```
does this server conform to the pinned HTTP profile whose request parsing
and proxy-boundary invariants rule out the smuggling ambiguity I care about?
```

The answer can be a CID.

That does not eliminate bugs in HTTP implementations. It changes the form of the claim. Instead of "we implement HTTP," the implementation publishes witnessed roots for specific obligations under specific policies.

The refusals matter as much as the positive witnesses:

```
does not support obsolete line folding
refuses duplicate Content-Length mismatch
refuses Transfer-Encoding + Content-Length ambiguity
refuses invalid header field syntax
does not claim HTTP/2 priority behavior
```

Refusal is not failure. Refusal is explicit protocol shape. A protocol profile with honest refusals is more useful than a broad conformance claim that silently inherits every ambiguity in the historical standard.

## Section 5: `.proof` for TCP

TCP makes the same point harder, because TCP is temporal.

A `.proof` for TCP cannot be only a packet grammar. The packet grammar is necessary but not sufficient. The protocol's load-bearing obligations live in state transitions and traces.

Example profile:

```
tcpSpecCid
tcpSegmentGrammarCid
tcpStateMachineCid
tcpSequenceArithmeticInvariantCid
tcpHandshakeInvariantCid
tcpRetransmissionPolicyCid
tcpFlowControlInvariantCid
tcpTeardownInvariantCid
tcpOptionNegotiationGrammarCid
tcpTraceProjectionCid
tcpPolicyCid
  -> tcpProfileRootCid
```

An implementation witness might bind:

```
kernelBuildCid
tcpProfileRootCid
adapterCid
symbolicStateMachineWitnessCid
packetTraceCorpusCid
traceConformanceWitnessCid
fuzzWitnessCid
refusalCid
  -> kernelTcpConformanceWitnessCid
```

This is not a claim that all future packets will behave correctly under all network conditions. It is a claim that a particular implementation, under a particular profile and policy, has witnessed conformance over named symbolic obligations, trace projections, corpora, and refusals.

The difference matters.

TCP implementations already differ. Congestion control differs. Timer behavior differs. Option support differs. Middlebox interactions differ. Kernel versions differ. Embedded stacks differ. The substrate does not pretend that "TCP" is one uniform object. It lets each profile be explicit:

```
this profile requires SACK
this profile refuses window scaling
this profile accepts these timer bounds
this profile witnesses this state-machine projection
this profile leaves this congestion-control behavior out of scope
```

That is protocol honesty.

In the traditional model, protocol variance is often discovered operationally: deploy, observe failures, read traces, compare implementations, file bugs, update documentation. In the substrate model, variance becomes part of the artifact graph.

## Section 6: Protocol migration as witnessed edges

Version changes are where the old model leaks most.

A protocol version changes from `v1` to `v2`. The changelog says "backward compatible except for X." Implementers update. Some clients lag. Some middleboxes ossify. Some servers run mixed behavior. Tooling adds feature flags. Docs accumulate tables. Security profiles fork. The standard moves forward; reality drags a graph behind it.

ProvekIt makes the graph explicit.

```
protocolV1RootCid
protocolV2RootCid
migrationBodyCid
migrationGrammarCid
compatibilityInvariantSetCid
bridgeCheckerCid
policyCid
  -> migrationWitnessCid
```

The migration body can say:

```
v2 preserves these v1 obligations
v2 strengthens these predicates
v2 weakens these predicates only under this opt-in profile
v2 refuses these legacy forms
v2 requires this new extension point grammar
v1 clients can interoperate through this adapter
v1 servers cannot safely interoperate under this policy
```

A consumer can then decide locally:

```
accept migrationWitnessCid
refuse migrationWitnessCid
pin protocolV1RootCid
pin protocolV2RootCid
accept only bridge profile B
accept only after signer S attests
```

This is the end of "upgrade because the prose says so" as the only available mechanism.

The prose still matters. Humans need the changelog. But the changelog can point at witnessed edges.

Protocol governance becomes verifiable supply chain data.

## Section 7: Adoption as a local policy decision

The substrate does not create one global truth about which protocol version everyone must accept.

That would be a different architecture.

The substrate creates local, explicit trust decisions over shared data.

A cloud provider may accept:

```
httpProfileRootCid_A
migrationWitnessCid_A_to_B
signerSetCid_cloud
```

A medical-device regulator may accept:

```
httpProfileRootCid_A
no migration to B until safetyWitnessCid exists
signerSetCid_regulator
```

A browser may accept:

```
httpProfileRootCid_B
legacyAdapterWitnessCid
requestSmugglingRefusalCid
```

An embedded controller may pin:

```
tcpProfileRootCid_legacy
refuse tcpProfileRootCid_new
```

These are not disagreements hidden in deployment reality. They are explicit policy artifacts.

The substrate's political move is subtle: it does not centralize protocol adoption. It makes adoption legible.

Every consumer can answer:

```
which protocol root did I rely on?
which bridge did I accept?
which signer did I trust?
which invariants did I require?
which claims did I refuse?
```

That is a better object for governance than "we support HTTP" or "we are moving to v2."

## Section 8: The self-hosting boundary

The protocol specs from May 6, 2026 create a recursive stack:

```
extension protocol
  -> formal grammar
    -> ProofIR invariant set
      -> GCP conformance body-claim
        -> TDP truth witness
          -> signed/CID-bearing letter
```

Then the body of the TDP witness can itself have a grammar. The grammar can itself have invariants. The GCP body can itself be GCP-conformance witnessed. This is the joke:

```
protocol spec inside protocol spec
conforming to protocol spec while conforming to protocol spec
```

But it is not a joke architecturally.

The recursion is stratified.

Core verification does not ask whether GCP is true because GCP says GCP is true. Core verification checks:

```
canonical bytes
CID
signature
references
core header validity
```

Everything else is an extension witness under policy.

That gives the useful form of self-hosting:

```
GCP can witness that a GCP-shaped body conforms to the GCP grammar
TDP can witness truth over a TDP-shaped body
CBP can run checker bytecode carried in signed body bytes
ORP can produce or transform bodies whose admissibility is witnessed
```

The protocol can carry evidence about itself without becoming circular because the evidence never becomes a core axiom.

This is the boundary:

```
self-describing: yes
self-witnessing: yes
self-validating core: no
```

That boundary is load-bearing.

## Section 9: What this changes about standards

Standards work today produces documents and conformance suites.

Documents are human-readable. Conformance suites are executable. Neither is usually the full protocol reality. Documents contain ambiguities. Suites are incomplete. Implementations become de facto standards. Errata accumulate. Security profiles fork. Backward compatibility becomes institutional memory.

The substrate does not replace standards bodies. It gives them a new artifact to publish.

A standards group can publish:

```
spec prose CID
normative grammar CID
normative invariant set CID
official parser/checker CIDs
official conformance policy CID
official test corpus CID
known ambiguity register CID
version bridge witness CIDs
deprecated behavior refusal CIDs
```

A reference implementation can publish:

```
implementation CID
adapter CID
grammar conformance witness
state-machine conformance witness
test corpus witness
known-refusal witness
```

An independent lab can publish:

```
third-party conformance witness
counterexample witness
interop witness
policy-specific refusal
```

This does not make the standards process less human. It makes the output less slippery.

Disagreement becomes structured:

```
we accept the grammar but reject the invariant
we accept the v1->v2 bridge under policy A but not policy B
we accept the syntax profile but refuse the state-machine profile
we accept the reference parser only through parserConformanceWitnessCid
```

That is better than prose disagreement because it can be pinned, compared, and audited.

## Section 10: Why this is not "formal methods for everything"

The obvious objection is that this sounds like formalizing the entire world.

It is not.

The substrate does not require every protocol to have a complete formal semantics before it can be useful. It allows protocol reality to become incrementally witnessable.

A protocol profile may begin with:

```
messageGrammarCid
parserCid
grammarConformanceWitnessCid
```

Later it may add:

```
stateMachineCid
stateMachineWitnessCid
```

Later:

```
securityInvariantSetCid
securityWitnessCid
```

Later:

```
migrationWitnessCid
interopWitnessCid
refusalCid
```

Partial formalization is not failure. It is honest scope.

The key is that the scope is named. A grammar-only witness does not pretend to prove state-machine behavior. A state-machine witness does not pretend to prove timing behavior. A test-corpus witness does not pretend to be exhaustive. A refusal is a first-class record that some claim is not being made.

This is how formal methods become adoptable at protocol scale: not by demanding total formalization up front, but by making each formalized boundary a signed, composable object.

## Section 11: Counterarguments

**"Protocols are too ambiguous for this."** Some are. That is precisely why ambiguity should be an artifact. A protocol profile can include `knownAmbiguityCid` and refusal edges. The substrate does not erase ambiguity; it prevents ambiguity from hiding inside a broad conformance claim.

**"Real implementations depend on de facto behavior, not specs."** Correct. De facto behavior can be witnessed too. An implementation trace corpus, interop suite, or observed compatibility profile can become a content-addressed artifact. A de facto profile is not a formal standard, but it is still a graph of claims.

**"A parser can lie."** Yes. That is why a relied-on parser should usually arrive as a witnessed root, not merely a raw parser CID. A policy can require parser conformance witnesses before accepting grammar conformance witnesses. The recursion is allowed; the base kernel remains finite.

**"A proof checker can be wrong."** Yes. A proof checker is an artifact under policy. Different consumers may accept different checker CIDs or require independent witnesses. The substrate makes that trust decision explicit. It does not remove the need for trust anchors.

**"This is circular when GCP witnesses GCP."** No. It would be circular only if core verification depended on GCP's result. It does not. Core validates bytes, CIDs, signatures, references, and finite header rules. GCP's self-conformance witness is evidence above the base kernel, not a core axiom.

**"This will fragment protocols into too many profiles."** Protocols are already fragmented into profiles. The substrate names them. A named fragment is better than an implicit one.

**"Standards bodies will not publish this."** Some will not at first. Independent labs, open-source projects, security teams, cloud providers, regulators, and package ecosystems can publish their own profile roots. The substrate is federated; official status is a policy input, not a substrate requirement.

**"This is too much data."** Protocol reality already generates the data: specs, tests, traces, errata, compatibility matrices, CVEs, bug trackers, implementation notes, and migration guides. The substrate changes the shape from scattered documents to signed DAGs.

## Section 12: The new unit of protocol governance

The old unit of protocol governance is the specification document.

The new unit is the witnessed protocol root.

That root can include prose. It can include formal grammar. It can include invariants. It can include parsers, checkers, tests, traces, refusals, bridges, migrations, and supersession edges. It can carry official signatures and adversarial signatures. It can be accepted by one policy and refused by another.

This does not reduce governance to math. It gives governance better objects.

Instead of:

```
Which version of the protocol do you support?
```

the ecosystem can ask:

```
Which protocol root do you rely on?
Which profile does it name?
Which grammar witness did you accept?
Which invariant witness did you accept?
Which migration edge did you accept?
Which refusals are in scope?
Which signer set did your policy trust?
```

Those questions are answerable by CIDs.

## Section 13: What this paper is NOT

This paper is not a claim that HTTP, TCP, TLS, DNS, or any other existing protocol can be fully formalized overnight.

It is not a claim that prose specifications become obsolete.

It is not a claim that one central authority should decide which protocol roots are valid.

It is not a claim that tests, traces, fuzzing, interop labs, or human review disappear.

It is not a claim that core verification executes protocol semantics.

It is a claim that the products of protocol work can become signed, content-addressed, grammar-constrained, invariant-constrained, witnessable artifacts, and that the relationships among those artifacts can themselves be witnessed.

That is enough.

## Section 14: After protocol specs

"After protocol specs" has two meanings.

First, this paper comes after the May 6 protocol specs: Extension Protocols, TDP, GCP, CBP, and ORP. Those specs define the machinery.

Second, it names the world after protocol specs stop being only documents.

In that world:

```
protocol versions are roots
protocol migrations are edges
protocol conformance is witnessed
protocol refusal is explicit
protocol adoption is local policy
protocol evolution is data
```

The internet has always evolved by accumulating compatibility facts. ProvekIt gives those facts a substrate.

The deep consequence is simple:

**A protocol change is not an instruction to upgrade your trust. It is a new artifact whose relationship to prior artifacts can itself be witnessed.**

That is how protocols actually evolve.

## Section 15: Citation

Cite as:

> ProvekIt Papers (2026). *After Protocol Specs: How Protocols Actually Evolve*. Draft whitepaper.
