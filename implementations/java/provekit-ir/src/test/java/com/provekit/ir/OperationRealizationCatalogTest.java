package com.provekit.ir;

import static org.junit.jupiter.api.Assertions.assertEquals;
import org.junit.jupiter.api.Test;

/**
 * Catalog-driven dispatch (#1391): the realizations catalog must contain
 * the six operation-realization mementos minted in this PR. Forward
 * (concept → rhs op) and reverse (rhs op → concept) must round-trip on
 * the java side.
 */
class OperationRealizationCatalogTest {

    @Test
    void operationRealizationCatalogRoundTrips() {
        String[][] probes = new String[][] {
            {"concept:utf8-encode",         "java:string-getBytes-utf8"},
            {"concept:json-text-coerce",    "java:jackson-jsonnode-asText"},
            {"concept:option-is-some",      "java:objects-nonnull"},
            {"concept:list-create",         "java:array-list-new"},
            {"concept:map-create",          "java:hashmap-new"},
            {"concept:format-string-interp","java:string-format-static"},
        };
        for (String[] pair : probes) {
            String concept = pair[0];
            String rhs = pair[1];
            assertEquals(rhs, OperationRealizationCatalog.javaOpFor(concept),
                    "forward lookup for " + concept);
            assertEquals(concept, OperationRealizationCatalog.conceptForJavaOp(rhs),
                    "reverse lookup for " + rhs);
        }
    }
}
