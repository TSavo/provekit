package com.provekit.lsp;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class ForwardPropagatorTests {
    @Test
    void callsiteSatisfiesPre_noDiagnostic() {
        ForwardPropagator fp = new ForwardPropagator();
        fp.addToCatalog("checkPositive", 
            new ForwardPropagator.Post(java.util.List.of("x > 0"), false),
            new ForwardPropagator.Post(java.util.List.of("x <= 0"), false));
        
        var currentPost = new ForwardPropagator.Post(java.util.List.of("x > 0"), false);
        var result = fp.checkCallsite("checkPositive", currentPost);
        
        assertNull(result);
    }
    
    @Test
    void callsiteViolatesPre_diagnosticEmitted() {
        ForwardPropagator fp = new ForwardPropagator();
        fp.addToCatalog("checkPositive", 
            new ForwardPropagator.Post(java.util.List.of("x > 0"), false),
            new ForwardPropagator.Post(java.util.List.of("x <= 0"), false));
        
        var currentPost = new ForwardPropagator.Post(java.util.List.of("x <= 0"), false);
        var result = fp.checkCallsite("checkPositive", currentPost);
        
        assertNotNull(result);
        assertEquals("implication-failed", result.code);
    }
    
    @Test
    void topFallback_noDiagnostic() {
        ForwardPropagator fp = new ForwardPropagator();
        
        var currentPost = new ForwardPropagator.Post(java.util.List.of(), true);
        var result = fp.checkCallsite("checkPositive", currentPost);
        
        assertNull(result);
    }
}