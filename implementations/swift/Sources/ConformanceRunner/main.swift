import Foundation
import Provekit

let args = CommandLine.arguments

if args.count > 1 && args[1] == "--fixture" {
    let name = args.count > 2 ? args[2] : ""
    switch name {
    case "eq_atomic":
        let lhs = Term.ctor("parse_int", Term.str("42"))
        let rhs = Term.num(42)
        let f = Formula.eq(lhs, rhs)
        print(Jcs.encode(Jcs.formulaToValue(f)), terminator: "")
    case "pattern1_bounded_loop":
        let x = Term.var(name: "x")
        let z = Term.num(0)
        let h = Term.num(100)
        let body = Formula.implies(Formula.and(Formula.gte(x, z), Formula.lt(x, h)), Formula.gte(x, z))
        let q = Formula.forall(name: "x", sort: .int, body: body)
        print(Jcs.encode(Jcs.formulaToValue(q)), terminator: "")
    default:
        break
    }
    exit(0)
}

func test(_ name: String, block: () -> Bool) {
    if block() {
        print("PASS: \(name)")
    } else {
        print("FAIL: \(name)")
        exit(1)
    }
}

let lhs = Term.ctor("parse_int", Term.str("42"))
let rhs = Term.num(42)
let f = Formula.eq(lhs, rhs)
let jcs1 = Jcs.encode(Jcs.formulaToValue(f))
let expected1 = #"{"args":[{"args":[{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"42"}],"kind":"ctor","name":"parse_int"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":42}],"kind":"atomic","name":"="}"#
if jcs1 != expected1 { print("GOT:  \(jcs1)"); print("EXP:  \(expected1)") }
test("eq_atomic JCS") { jcs1 == expected1 }

let hash1 = Blake3.hex(Data(jcs1.utf8))
test("eq_atomic hash") { hash1 == "blake3-512:5eade72c08811b2d38adcb158eced38f3d319de090d59b2fa7a77ad830169e18539d2b75d2a2838c545e644a688cf137603674523ff37f1586a650f6dd05aeaa" }

let x = Term.var(name: "x")
let z = Term.num(0)
let h = Term.num(100)
let lower = Formula.gte(x, z)
let upper = Formula.lt(x, h)
let ant = Formula.and(lower, upper)
let inner = Formula.gte(x, z)
let body = Formula.implies(ant, inner)
let q = Formula.forall(name: "x", sort: .int, body: body)
let jcs3 = Jcs.encode(Jcs.formulaToValue(q))
let expected3 = #"{"body":{"kind":"implies","operands":[{"kind":"and","operands":[{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"},{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":100}],"kind":"atomic","name":"<"}]},{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"}]},"kind":"forall","name":"x","sort":{"kind":"primitive","name":"Int"}}"#
test("pattern1 JCS") { jcs3 == expected3 }

let x2 = Term.var(name: "x")
let z2 = Term.num(0)
let pre = Formula.gte(x2, z2)
let d = Declaration.contract(name: "parseInt", outBinding: "out", pre: pre, post: nil, inv: nil)
let jcs4 = Jcs.encodeDeclarations([d])
let expected4 = #"[{"kind":"contract","name":"parseInt","outBinding":"out","pre":{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"}}]"#
test("contract JCS") { jcs4 == expected4 }

let b = Declaration.bridge(
    name: "myBridge", sourceSymbol: "source", sourceLayer: "c-kit",
    sourceContractCid: "bafySource", targetContractCid: "bafyTarget",
    targetProofCid: "bafyProof", targetLayer: "coq", notes: "some notes"
)
let jcs5 = Jcs.encodeDeclarations([b])
let expected5 = #"[{"kind":"bridge","name":"myBridge","notes":"some notes","sourceContractCid":"bafySource","sourceLayer":"c-kit","sourceSymbol":"source","targetContractCid":"bafyTarget","targetLayer":"coq","targetProofCid":"bafyProof"}]"#
if jcs5 != expected5 { print("GOT:  \(jcs5)"); print("EXP:  \(expected5)") }
test("bridge JCS") { jcs5 == expected5 }

// MARK: - Phase 2 cross-kit bridges (lift-plugin-protocol)
//
// 10 counterpart contracts + 10 bridges to the rust kit's lift-plugin-
// protocol contracts. See Provekit/CrossKitBridges.swift for the slab.
//
// Pinned BLAKE3-512 of the JCS-canonical bytes of the 10 BridgeDeclarations
// returned by CrossKitBridges.buildAllBridges(). Drift in any rust contract
// CID, counterpart formula shape, bridge field, JCS emitter, or declaration
// order will fail this assertion with a clear next step.
//
// Verified against rust contract CIDs extracted from
//   cargo run --release -p provekit-self-contracts \
//     --bin print-lift-plugin-protocol-cids
// and cross-kit-pinned in implementations/{python,go,typescript}.

let allBridges = CrossKitBridges.buildAllBridges()
test("phase2 bridges count") { allBridges.count == 10 }

let allDecls = CrossKitBridges.buildAllDeclarations()
test("phase2 declarations count") { allDecls.count == 20 }

// Each bridge must carry the rust source CID for its named rust contract.
test("phase2 bridges source CIDs") {
    for (i, name) in CrossKitBridges.liftPluginProtocolNames.enumerated() {
        guard case .bridge(let bn, let ss, let sl, let scid, _, let tpc, let tl, let notes) = allBridges[i] else {
            return false
        }
        if bn != "bridge_to_\(name)" { return false }
        if ss != name { return false }
        if sl != "rust-kit" { return false }
        if tl != "swift-kit" { return false }
        if tpc != "deferred:phase-3-proof-bundle" { return false }
        if notes != "lift-plugin-protocol conformance bridge; phase 2" { return false }
        if scid != CrossKitBridges.rustContractCids[name] { return false }
    }
    return true
}

// Each bridge's targetContractCid must equal the JCS-hash of its paired
// counterpart (decls layout is [c0, b0, c1, b1, ...]).
test("phase2 bridge targets paired counterparts") {
    for i in stride(from: 0, to: allDecls.count, by: 2) {
        let cp = allDecls[i]
        let br = allDecls[i + 1]
        let expected = CrossKitBridges.declarationCid(cp)
        guard case .bridge(_, _, _, _, let tcid, _, _, _) = br else { return false }
        if tcid != expected { return false }
    }
    return true
}

let bridgesJcs = Jcs.encodeDeclarations(allBridges)
let bridgesHash = Blake3.hex(Data(bridgesJcs.utf8))
let expectedBridgesHash =
    "blake3-512:d1be24c33a873052e9e1487e152ccf0c2c2d6580f43325be5b86557ac920475c473ea031c9a5731317f72e3168755aa89f1cbb295b3e24814d8e2d019473e1ac"
if bridgesHash != expectedBridgesHash {
    print("GOT:  \(bridgesHash)")
    print("EXP:  \(expectedBridgesHash)")
}
test("phase2 bridges pinned hash") { bridgesHash == expectedBridgesHash }

print("ALL PASS")
