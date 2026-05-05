// SPDX-License-Identifier: Apache-2.0
//
// ClaimEnvelope — mints signed contract / bridge ClaimEnvelopes.
//
// Mirrors the go reference at
// implementations/go/provekit-ir-symbolic/claim_envelope/. v1.1.0 cut:
//   * BLAKE3-512 with self-identifying "blake3-512:" prefix everywhere;
//   * "ed25519:" + base64(sig) producer signatures;
//   * bindingHash and propertyHash are DERIVED inside the minter
//     (callers MUST NOT supply them);
//   * canonical-input bytes for both CID and signature are
//     `JCS(envelope minus cid + producerSignature)`.
//
// Spec:
//   protocol/specs/2026-04-29-universal-claim-envelope.md
//   protocol/specs/2026-04-30-memento-envelope-grammar.md

import Foundation

// MARK: - Constants

public enum ClaimEnvelopeSchema {
    /// Pinned schema CIDs from the go/rust references. Stable across
    /// every conformant kit; producers MUST emit exactly these.
    public static let contract =
        "blake3-512:00000000000000000000000000000000000000000000000000000000000000d000000000000000000000000000000000000000000000000000000000000000d0"
    public static let bridge =
        "blake3-512:00000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000c0"
    public static let implication =
        "blake3-512:00000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000000e0"
}

public enum Verdict: String, Sendable {
    case holds
    case violated
    case decayed
    case undecidable
    case error
}

/// Authoring discriminator. Mirrors the `producerKind` enum in the
/// memento envelope grammar.
public enum AuthoringKind: Sendable {
    case kitAuthor(author: String, note: String?)
    case lift(lifter: String, evidence: String, sourceCid: String?)
    case llm(llm: String, llmVersion: String, promptCid: String,
             confidence: Double, rationale: String?)
}

/// One minted memento: the final signed JCS bytes + the envelope CID.
public struct MintedMemento: Sendable, Equatable {
    public let canonicalBytes: Data
    public let cid: String

    public init(canonicalBytes: Data, cid: String) {
        self.canonicalBytes = canonicalBytes
        self.cid = cid
    }
}

// MARK: - Errors

public enum ClaimEnvelopeError: Error, CustomStringConvertible {
    case missingFormulae
    case missingOutBinding
    case missingContractName
    case missingSourceSymbol
    case missingSourceLayer
    case missingTargetContractCid

    public var description: String {
        switch self {
        case .missingFormulae:
            return "MintContract: at least one of pre/post/inv must be non-nil"
        case .missingOutBinding:
            return "MintContract: outBinding is required"
        case .missingContractName:
            return "MintContract: contractName is required"
        case .missingSourceSymbol:
            return "MintBridge: sourceSymbol is required"
        case .missingSourceLayer:
            return "MintBridge: sourceLayer is required"
        case .missingTargetContractCid:
            return "MintBridge: targetContractCid is required"
        }
    }
}

// MARK: - Mint args

public struct ContractMintArgs: Sendable {
    public var contractName: String
    public var pre: JcsCanonical?
    public var post: JcsCanonical?
    public var inv: JcsCanonical?
    public var outBinding: String
    public var producedBy: String
    public var producedAt: String
    public var inputCids: [String]
    public var authoring: AuthoringKind

    public init(
        contractName: String,
        pre: JcsCanonical? = nil,
        post: JcsCanonical? = nil,
        inv: JcsCanonical? = nil,
        outBinding: String,
        producedBy: String,
        producedAt: String,
        inputCids: [String] = [],
        authoring: AuthoringKind
    ) {
        self.contractName = contractName
        self.pre = pre
        self.post = post
        self.inv = inv
        self.outBinding = outBinding
        self.producedBy = producedBy
        self.producedAt = producedAt
        self.inputCids = inputCids
        self.authoring = authoring
    }
}

public struct BridgeMintArgs: Sendable {
    public var producedBy: String
    public var producedAt: String
    public var sourceSymbol: String
    public var sourceLayer: String
    public var sourceContractCid: String
    public var targetContractCid: String
    public var targetProofCid: String
    public var targetLayer: String
    public var irArgSorts: [JcsCanonical]
    public var irReturnSort: JcsCanonical
    public var notes: String

    public init(
        producedBy: String,
        producedAt: String,
        sourceSymbol: String,
        sourceLayer: String,
        sourceContractCid: String = "",
        targetContractCid: String,
        targetProofCid: String = "",
        targetLayer: String,
        irArgSorts: [JcsCanonical] = [],
        irReturnSort: JcsCanonical = .string("Bool"),
        notes: String = ""
    ) {
        self.producedBy = producedBy
        self.producedAt = producedAt
        self.sourceSymbol = sourceSymbol
        self.sourceLayer = sourceLayer
        self.sourceContractCid = sourceContractCid
        self.targetContractCid = targetContractCid
        self.targetProofCid = targetProofCid
        self.targetLayer = targetLayer
        self.irArgSorts = irArgSorts
        self.irReturnSort = irReturnSort
        self.notes = notes
    }
}

// MARK: - v1.4 Bridge types

/// Tagged-union target per 2026-05-03-bridge-target-dimensionality.md §1.R1.
public enum BridgeTargetKind: String, Sendable { case contract, contractSet }

public struct BridgeTarget: Sendable {
    public let kind: BridgeTargetKind
    public let cid: String

    public init(kind: BridgeTargetKind, cid: String) {
        self.kind = kind
        self.cid = cid
    }
}

/// v1.4 bridge mint inputs. Metadata fields are optional (nil = omit per §1.R2).
public struct BridgeMintV14Args: Sendable {
    public var name: String
    public var sourceSymbol: String
    public var sourceLayer: String
    public var sourceContractCid: String
    public var target: BridgeTarget

    public var targetWitnessCid: String?
    public var targetBinaryCid: String?
    public var targetLayer: String?
    public var targetContractSetCid: String?
    public var producedBy: String?
    public var producedAt: String?

    public var declaredAt: String

    public init(
        name: String, sourceSymbol: String, sourceLayer: String,
        sourceContractCid: String, target: BridgeTarget,
        declaredAt: String
    ) {
        self.name = name
        self.sourceSymbol = sourceSymbol
        self.sourceLayer = sourceLayer
        self.sourceContractCid = sourceContractCid
        self.target = target
        self.declaredAt = declaredAt
    }
}

// MARK: - Minter

/// Stateful envelope builder bound to one signing seed. The kit's mints
/// per run use a single Minter; rotate by constructing a new one.
public struct ClaimMinter: Sendable {
    public let signerSeed: [UInt8]

    public init(signerSeed: [UInt8]) {
        precondition(signerSeed.count == 32, "Ed25519 seed must be 32 bytes")
        self.signerSeed = signerSeed
    }

    /// Mint a v1.1.0 contract memento.
    public func mintContract(_ args: ContractMintArgs) throws -> MintedMemento {
        if args.pre == nil && args.post == nil && args.inv == nil {
            throw ClaimEnvelopeError.missingFormulae
        }
        if args.outBinding.isEmpty { throw ClaimEnvelopeError.missingOutBinding }
        if args.contractName.isEmpty { throw ClaimEnvelopeError.missingContractName }

        // body: locked field set per memento envelope grammar.
        var bodyPairs: [(String, JcsCanonical)] = [
            ("contractName", .string(args.contractName)),
            ("outBinding", .string(args.outBinding)),
        ]
        if let pre = args.pre {
            bodyPairs.append(("pre", pre))
            bodyPairs.append(("preHash", .string(hashValue(pre))))
        }
        if let post = args.post {
            bodyPairs.append(("post", post))
            bodyPairs.append(("postHash", .string(hashValue(post))))
        }
        if let inv = args.inv {
            bodyPairs.append(("inv", inv))
            bodyPairs.append(("invHash", .string(hashValue(inv))))
        }
        bodyPairs.append(("authoring", buildAuthoring(args.authoring)))

        let evidence: JcsCanonical = .object([
            ("kind", .string("contract")),
            ("schema", .string(ClaimEnvelopeSchema.contract)),
            ("body", .object(bodyPairs)),
        ])

        // propertyHash = ComputeCID(JCS({pre?, post?, inv?, outBinding}))
        var phPairs: [(String, JcsCanonical)] = [
            ("outBinding", .string(args.outBinding)),
        ]
        if let pre = args.pre { phPairs.append(("pre", pre)) }
        if let post = args.post { phPairs.append(("post", post)) }
        if let inv = args.inv { phPairs.append(("inv", inv)) }
        let propertyHash = hashValue(.object(phPairs))

        // bindingHash = ComputeCID(JCS({producerId, contractName, propertyHash}))
        let bindingHash = hashValue(.object([
            ("producerId", .string(args.producedBy)),
            ("contractName", .string(args.contractName)),
            ("propertyHash", .string(propertyHash)),
        ]))

        let unsigned = envelopeForHashing(
            bindingHash: bindingHash,
            propertyHash: propertyHash,
            verdict: .holds,
            producedBy: args.producedBy,
            producedAt: args.producedAt,
            inputCids: args.inputCids,
            evidence: evidence
        )
        return finalize(unsigned: unsigned)
    }

    /// Mint a v1.1.0 bridge memento.
    public func mintBridge(_ args: BridgeMintArgs) throws -> MintedMemento {
        if args.sourceSymbol.isEmpty { throw ClaimEnvelopeError.missingSourceSymbol }
        if args.sourceLayer.isEmpty { throw ClaimEnvelopeError.missingSourceLayer }
        if args.targetContractCid.isEmpty { throw ClaimEnvelopeError.missingTargetContractCid }

        var bodyPairs: [(String, JcsCanonical)] = [
            ("sourceSymbol", .string(args.sourceSymbol)),
            ("sourceLayer", .string(args.sourceLayer)),
            ("targetContractCid", .string(args.targetContractCid)),
            ("targetLayer", .string(args.targetLayer)),
            ("irArgSorts", .array(args.irArgSorts)),
            ("irReturnSort", args.irReturnSort),
        ]
        if !args.sourceContractCid.isEmpty {
            bodyPairs.append(("sourceContractCid", .string(args.sourceContractCid)))
        }
        if !args.targetProofCid.isEmpty {
            bodyPairs.append(("targetProofCid", .string(args.targetProofCid)))
        }
        if !args.notes.isEmpty {
            bodyPairs.append(("notes", .string(args.notes)))
        }

        let evidence: JcsCanonical = .object([
            ("kind", .string("bridge")),
            ("schema", .string(ClaimEnvelopeSchema.bridge)),
            ("body", .object(bodyPairs)),
        ])

        let bindingHash = hashValue(.object([
            ("sourceLayer", .string(args.sourceLayer)),
            ("sourceSymbol", .string(args.sourceSymbol)),
        ]))
        // bridge propertyHash is the literal-string content-address
        // ("bridge:" + sourceSymbol), NOT a JCS-wrapped value. Mirrors
        // the go and rust kit derivation.
        let propertyHash = Blake3.hex(
            Data(("bridge:" + args.sourceSymbol).utf8)
        )

        let unsigned = envelopeForHashing(
            bindingHash: bindingHash,
            propertyHash: propertyHash,
            verdict: .holds,
            producedBy: args.producedBy,
            producedAt: args.producedAt,
            inputCids: [args.targetContractCid],
            evidence: evidence
        )
        return finalize(unsigned: unsigned)
    }

    // ----- v1.4 bridge (layered envelope/header/body, tagged-union target) -----

    public func mintBridgeV14(_ args: BridgeMintV14Args) throws -> MintedMemento {
        if args.name.isEmpty { throw ClaimEnvelopeError.missingContractName }

        // Build target
        let target: JcsCanonical = .object([
            ("kind", .string(args.target.kind.rawValue)),
            ("cid", .string(args.target.cid)),
        ])

        // Build header (7 canonical fields per spec §1.R3)
        let header: JcsCanonical = .object([
            ("schemaVersion", .string("1")),
            ("kind", .string("bridge")),
            ("name", .string(args.name)),
            ("sourceSymbol", .string(args.sourceSymbol)),
            ("sourceLayer", .string(args.sourceLayer)),
            ("sourceContractCid", .string(args.sourceContractCid)),
            ("target", target),
        ])

        // Build metadata (omit nil fields per §1.R2)
        var metaPairs: [(String, JcsCanonical)] = []
        if let v = args.targetWitnessCid     { metaPairs.append(("targetWitnessCid", .string(v))) }
        if let v = args.targetBinaryCid      { metaPairs.append(("targetBinaryCid", .string(v))) }
        if let v = args.targetLayer          { metaPairs.append(("targetLayer", .string(v))) }
        if let v = args.targetContractSetCid { metaPairs.append(("targetContractSetCid", .string(v))) }
        if let v = args.producedBy           { metaPairs.append(("producedBy", .string(v))) }
        if let v = args.producedAt           { metaPairs.append(("producedAt", .string(v))) }
        let meta: JcsCanonical = .object(metaPairs)

        // Sign: JCS({header, metadata})
        let sigPayload: JcsCanonical = .object([
            ("header", header),
            ("metadata", meta),
        ])
        let sigPayloadBytes = JcsCanonicalizer.encode(sigPayload)
        let sigStr = Ed25519.signatureString(message: sigPayloadBytes, seed: signerSeed)

        // Build envelope
        let pubkey = Ed25519.publicKeyString(fromSeed: signerSeed)
        let envelope: JcsCanonical = .object([
            ("signer", .string(pubkey)),
            ("declaredAt", .string(args.declaredAt)),
            ("signature", .string(sigStr)),
        ])

        // Full memento: {envelope, header, metadata}
        let memento: JcsCanonical = .object([
            ("envelope", envelope),
            ("header", header),
            ("metadata", meta),
        ])
        let canonical = JcsCanonicalizer.encode(memento)
        let cid = Blake3.hex(canonical)

        return MintedMemento(canonicalBytes: canonical, cid: cid)
    }

    // ----- helpers -----

    /// Build the canonical-input value (envelope minus cid +
    /// producerSignature). Hashing AND signing operate on JCS bytes of
    /// this value.
    private func envelopeForHashing(
        bindingHash: String,
        propertyHash: String,
        verdict: Verdict,
        producedBy: String,
        producedAt: String,
        inputCids: [String],
        evidence: JcsCanonical
    ) -> JcsCanonical {
        let sortedInputs = inputCids.sorted()
        return .object([
            ("schemaVersion", .string("1")),
            ("bindingHash", .string(bindingHash)),
            ("propertyHash", .string(propertyHash)),
            ("verdict", .string(verdict.rawValue)),
            ("producedBy", .string(producedBy)),
            ("producedAt", .string(producedAt)),
            ("inputCids", .array(sortedInputs.map { .string($0) })),
            ("evidence", evidence),
        ])
    }

    private func finalize(unsigned: JcsCanonical) -> MintedMemento {
        let canonical = JcsCanonicalizer.encode(unsigned)
        let cid = Blake3.hex(canonical)
        let sigStr = Ed25519.signatureString(message: canonical, seed: signerSeed)

        // Rebuild as a signed object. Take the unsigned object's pairs
        // and append producerSignature + cid; emit re-canonicalized.
        guard case .object(let pairs) = unsigned else {
            // unreachable: envelopeForHashing always returns an object
            preconditionFailure("envelopeForHashing must return an object")
        }
        var signed = pairs
        signed.append(("producerSignature", .string(sigStr)))
        signed.append(("cid", .string(cid)))
        let finalBytes = JcsCanonicalizer.encode(.object(signed))
        return MintedMemento(canonicalBytes: finalBytes, cid: cid)
    }
}

// MARK: - Helpers shared with the orchestrator

/// `hashValue(v) := ComputeCID(JCS(v))` — the v1.1.0 protocol's standard
/// content-address used for preHash / postHash / invHash, propertyHash,
/// and bindingHash.
public func hashValue(_ v: JcsCanonical) -> String {
    return Blake3.hex(JcsCanonicalizer.encode(v))
}

/// Build the authoring block. Each variant is a typed-union with
/// `producerKind` as the discriminator, mirroring the go reference.
private func buildAuthoring(_ k: AuthoringKind) -> JcsCanonical {
    switch k {
    case .kitAuthor(let author, let note):
        var pairs: [(String, JcsCanonical)] = [
            ("producerKind", .string("kit-author")),
            ("author", .string(author)),
        ]
        if let note = note { pairs.append(("note", .string(note))) }
        return .object(pairs)

    case .lift(let lifter, let evidence, let sourceCid):
        var pairs: [(String, JcsCanonical)] = [
            ("producerKind", .string("lift")),
            ("lifter", .string(lifter)),
            ("evidence", .string(evidence)),
        ]
        if let sourceCid = sourceCid {
            pairs.append(("sourceCid", .string(sourceCid)))
        }
        return .object(pairs)

    case .llm(let llm, let llmVersion, let promptCid, let confidence, let rationale):
        var pairs: [(String, JcsCanonical)] = [
            ("producerKind", .string("llm")),
            ("llm", .string(llm)),
            ("llmVersion", .string(llmVersion)),
            ("promptCid", .string(promptCid)),
            // Encoded losslessly as integer permille for v0; matches go.
            ("confidence", .int(Int64(confidence * 1000))),
        ]
        if let rationale = rationale {
            pairs.append(("rationale", .string(rationale)))
        }
        return .object(pairs)
    }
}

// MARK: - contractCid + contractSetCid (spec #94 §1)

/// Compute the signer-independent contractCid.
///
///     contractCid := blake3-512(JCS({name, outBinding, pre?, post?, inv?}))
///
/// Two distinct signers attesting to the same logical contract produce
/// the same contractCid. NOT the attestation CID (envelope hash).
public func contractCid(fromArgs args: ContractMintArgs) -> String {
    var pairs: [(String, JcsCanonical)] = [
        ("name", .string(args.contractName)),
        ("outBinding", .string(args.outBinding)),
    ]
    if let pre = args.pre { pairs.append(("pre", pre)) }
    if let post = args.post { pairs.append(("post", post)) }
    if let inv = args.inv { pairs.append(("inv", inv)) }
    return Blake3.hex(JcsCanonicalizer.encode(.object(pairs)))
}

/// Compute the contract set CID per spec 2026-05-03-contract-set-extension.md §1:
///
///     contractSetCid := blake3-512(JCS(<sorted contractCids>))
///
/// The sort is lexicographic on the raw "blake3-512:hex" strings so that
/// two kits enumerating the same contracts in different order produce
/// byte-identical contractSetCid values.
public func computeContractSetCid(_ contractCids: [String]) -> String {
    let sorted = contractCids.sorted()
    let arr: JcsCanonical = .array(sorted.map { .string($0) })
    return Blake3.hex(JcsCanonicalizer.encode(arr))
}
