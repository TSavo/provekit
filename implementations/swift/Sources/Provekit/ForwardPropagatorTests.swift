import Foundation
import XCTest
@testable import Provekit

final class ForwardPropagatorTests: XCTestCase {
    func test_callsiteSatisfiesPre_noDiagnostic() {
        let fp = ForwardPropagator()
        fp.addToCatalog("checkPositive", pre: .of("x > 0"), post: .of("x <= 0"))
        
        let currentPost = ForwardPropagator.Post.constraints(["x > 0"], isTop: false)
        let result = fp.checkCallsite("checkPositive", currentPost: currentPost)
        
        XCTAssertNil(result)
    }
    
    func test_callsiteViolatesPre_diagnosticEmitted() {
        let fp = ForwardPropagator()
        fp.addToCatalog("checkPositive", pre: .of("x > 0"), post: .of("x <= 0"))
        
        let currentPost = ForwardPropagator.Post.constraints(["x <= 0"], isTop: false)
        let result = fp.checkCallsite("checkPositive", currentPost: currentPost)
        
        XCTAssertNotNil(result)
        XCTAssertEqual(result?.code, "implication-failed")
    }
    
    func test_topFallback_noDiagnostic() {
        let fp = ForwardPropagator()
        fp.addToCatalog("checkPositive", pre: .of("x > 0"), post: .of("x <= 0"))
        
        let currentPost = ForwardPropagator.Post.constraints([], isTop: true)
        let result = fp.checkCallsite("checkPositive", currentPost: currentPost)
        
        XCTAssertNil(result)
    }
    
    func test_unknownCallee_noDiagnostic() {
        let fp = ForwardPropagator()
        
        let currentPost = ForwardPropagator.Post.constraints(["x > 0"], isTop: false)
        let result = fp.checkCallsite("unknown", currentPost: currentPost)
        
        XCTAssertNil(result)
    }
}