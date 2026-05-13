// SPDX-License-Identifier: Apache-2.0

package com.provekit.claimenvelope;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Set;
import java.util.TreeSet;

import org.junit.jupiter.api.Test;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.provekit.ir.Jcs;
import com.provekit.ir.Jcs.Value;

class CicpConformanceTest {
    private static final Path CICP_DIR = findCicpDir();
    private static final ObjectMapper JSON = new ObjectMapper();

    @Test
    void passing_vectors_derive_expected_blake3_512_cids() throws IOException {
        JsonNode catalog = JSON.readTree(CICP_DIR.resolve("vectors.json").toFile());

        for (JsonNode vector : catalog.path("vectors")) {
            if (!vector.path("shouldPass").asBoolean()) {
                continue;
            }

            JsonNode body = readBody(vector);
            assertEquals(List.of(), validateClosedInputs(body), vector.path("name").asText());
            assertEquals(
                vector.path("expectedCid").asText(),
                Jcs.blake3Cid(toJcsValue(body)),
                vector.path("name").asText());
        }
    }

    @Test
    void invalid_vectors_fail_closed_on_missing_input_cids() throws IOException {
        JsonNode catalog = JSON.readTree(CICP_DIR.resolve("vectors.json").toFile());

        for (JsonNode vector : catalog.path("vectors")) {
            if (vector.path("shouldPass").asBoolean()) {
                continue;
            }

            List<String> errors = validateClosedInputs(readBody(vector));
            assertFalse(errors.isEmpty(), vector.path("name").asText());
            assertTrue(
                errors.stream().anyMatch(error -> error.contains(vector.path("errorContains").asText())),
                vector.path("name").asText() + " errors: " + errors);
        }
    }

    private static JsonNode readBody(JsonNode vector) throws IOException {
        return JSON.readTree(CICP_DIR.resolve(vector.path("body").asText()).toFile());
    }

    private static Value toJcsValue(JsonNode node) {
        if (node.isObject()) {
            LinkedHashMap<String, Value> entries = new LinkedHashMap<>();
            node.fields().forEachRemaining(field -> entries.put(field.getKey(), toJcsValue(field.getValue())));
            return Value.object(entries);
        }
        if (node.isArray()) {
            List<Value> items = new ArrayList<>();
            node.forEach(item -> items.add(toJcsValue(item)));
            return Value.array(items);
        }
        if (node.isTextual()) {
            return Value.string(node.asText());
        }
        if (node.isIntegralNumber()) {
            return Value.integer(node.asLong());
        }
        if (node.isBoolean()) {
            return Value.bool(node.asBoolean());
        }
        if (node.isNull()) {
            return Value.NULL;
        }
        throw new IllegalArgumentException("Unsupported JSON node for JCS: " + node);
    }

    private static List<String> validateClosedInputs(JsonNode body) {
        Set<String> inputCids = textSet(body.path("inputCids"));
        Set<String> requiredCids = requiredInputCids(body);
        List<String> errors = new ArrayList<>();

        if (!body.path("inputCids").isArray()) {
            errors.add("inputCids missing required array");
            return errors;
        }
        for (String cid : requiredCids) {
            if (!inputCids.contains(cid)) {
                errors.add("inputCids missing required CID " + cid);
            }
        }
        return errors;
    }

    private static Set<String> requiredInputCids(JsonNode body) {
        Set<String> cids = new TreeSet<>();
        switch (body.path("kind").asText()) {
            case "CIBlastRadius" -> {
                addText(cids, body, "protocolCatalogCid");
                addText(cids, body, "jobDefinitionCid");
                addText(cids, body, "commandCid");
                addText(cids, body, "runnerIdentityCid");
                addText(cids, body, "sourceClosureCid");
                addText(cids, body, "policyCid");
                addTexts(cids, body, "toolchainCids");
                addTexts(cids, body, "lockfileCids");
                addTexts(cids, body, "generatedInputCids");
                addTexts(cids, body, "fixtureCids");
                addTexts(cids, body, "relevantSpecCids");
            }
            case "CIJobResultBodyClaim" -> {
                addText(cids, body, "blastRadiusCid");
                addText(cids, body, "outputCid");
                addText(cids, body, "logCid");
                addText(cids, body, "runnerIdentityCid");
                addText(cids, body, "policyCid");
            }
            case "CIReuseBodyClaim" -> {
                addText(cids, body, "currentBlastRadiusCid");
                addText(cids, body, "previousBlastRadiusCid");
                addText(cids, body, "previousResultWitnessCid");
                addText(cids, body, "policyCid");
                addTexts(cids, body, "bridgeWitnessCids");
            }
            case "CIImpactBodyClaim" -> {
                addText(cids, body, "baseStateCid");
                addText(cids, body, "candidateStateCid");
                addText(cids, body, "policyCid");
                addTexts(cids, body, "protocolEvolutionWitnessCids");
                addTexts(cids, body, "changedBlastRadiusCids");
                addTexts(cids, body, "unchangedBlastRadiusCids");
                addTexts(cids, body, "reusableWitnessCids");
                addTexts(cids, body, "refusalCids");
            }
            default -> throw new IllegalArgumentException("Unknown CICP body kind: " + body.path("kind").asText());
        }
        return cids;
    }

    private static Set<String> textSet(JsonNode node) {
        Set<String> values = new TreeSet<>();
        if (node.isArray()) {
            node.forEach(item -> {
                if (item.isTextual()) {
                    values.add(item.asText());
                }
            });
        }
        return values;
    }

    private static void addText(Set<String> cids, JsonNode body, String field) {
        JsonNode value = body.path(field);
        if (value.isTextual()) {
            cids.add(value.asText());
        }
    }

    private static void addTexts(Set<String> cids, JsonNode body, String field) {
        if (body.path(field).isArray()) {
            body.path(field).forEach(value -> {
                if (value.isTextual()) {
                    cids.add(value.asText());
                }
            });
        }
    }

    private static Path findCicpDir() {
        Path dir = Path.of("").toAbsolutePath();
        while (dir != null) {
            Path candidate = dir.resolve(Path.of("protocol", "conformance", "cicp"));
            if (Files.exists(candidate.resolve("vectors.json"))) {
                return candidate;
            }
            dir = dir.getParent();
        }
        throw new IllegalStateException("Could not locate protocol/conformance/cicp/vectors.json");
    }
}
