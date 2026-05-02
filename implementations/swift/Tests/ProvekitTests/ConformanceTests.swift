import XCTest
import Foundation
@testable import Provekit

final class ConformanceTests: XCTestCase {

    func testEqAtomicJcs() {
        let lhs = Term.ctor("parse_int", Term.str("42"))
        let rhs = Term.num(42)
        let f = Formula.eq(lhs, rhs)
        let jcs = Jcs.encodeFormula(f)
        let expected = #"{"args":[{"args":[{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"42"}],"kind":"ctor","name":"parse_int"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":42}],"kind":"atomic","name":"="}"#
        XCTAssertEqual(jcs, expected)
    }

    func testEqAtomicHash() {
        let lhs = Term.ctor("parse_int", Term.str("42"))
        let rhs = Term.num(42)
        let f = Formula.eq(lhs, rhs)
        let jcs = Jcs.encodeFormula(f)
        let hash = Blake3.hex(Data(jcs.utf8))
        let expected = "blake3-512:5eade72c08811b2d38adcb158eced38f3d319de090d59b2fa7a77ad830169e18539d2b75d2a2838c545e644a688cf137603674523ff37f1586a650f6dd05aeaa"
        XCTAssertEqual(hash, expected)
    }

    func testPattern1BoundedLoopJcs() {
        let x = Term.var(name: "x")
        let zero = Term.num(0)
        let hundred = Term.num(100)

        let lower = Formula.gte(x, zero)
        let upper = Formula.lt(x, hundred)
        let ant = Formula.and(lower, upper)
        let inner = Formula.gte(x, zero)
        let body = Formula.implies(ant, inner)
        let q = Formula.forall(name: "x", sort: .int, body: body)

        let jcs = Jcs.encodeFormula(q)
        let expected = #"{"body":{"kind":"implies","operands":[{"kind":"and","operands":[{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"},{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":100}],"kind":"atomic","name":"<"}]},{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"}]},"kind":"forall","name":"x","sort":{"kind":"primitive","name":"Int"}}"#
        XCTAssertEqual(jcs, expected)
    }

    func testContractDeclJcs() {
        let x = Term.var(name: "x")
        let zero = Term.num(0)
        let pre = Formula.gte(x, zero)
        let d = Declaration.contract(name: "parseInt", outBinding: "out", pre: pre, post: nil, inv: nil)
        let jcs = Jcs.encodeDeclarations([d])
        let expected = #"[{"kind":"contract","name":"parseInt","outBinding":"out","pre":{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"}}]"#
        XCTAssertEqual(jcs, expected)
    }

    func testBridgeDeclJcs() {
        let d = Declaration.bridge(
            name: "myBridge", sourceSymbol: "source", sourceLayer: "c-kit",
            sourceContractCid: "bafySource", targetContractCid: "bafyTarget",
            targetProofCid: "bafyProof", targetLayer: "coq", notes: "some notes"
        )
        let jcs = Jcs.encodeDeclarations([d])
        let expected = #"{"kind":"bridge","name":"myBridge","notes":"some notes","sourceContractCid":"bafySource","sourceLayer":"c-kit","sourceSymbol":"source","targetContractCid":"bafyTarget","targetLayer":"coq","targetProofCid":"bafyProof"}"#
        XCTAssertEqual(jcs, expected)
    }
}
