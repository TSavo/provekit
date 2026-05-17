package com.provekit.contract;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;

import java.util.Set;
import org.junit.jupiter.api.Test;

class RealizationTagsTest {
    private static final String COMPOSITION_TREE_CID =
            "blake3-512:1111111111111111111111111111111111111111111111111111111111111111"
                    + "1111111111111111111111111111111111111111111111111111111111111111";
    private static final String BOUNDARY_CONTRACT_CID =
            "blake3-512:2222222222222222222222222222222222222222222222222222222222222222"
                    + "2222222222222222222222222222222222222222222222222222222222222222";

    @Test
    void tagFirstClassBuildsFirstClassRealization() {
        RealizationTags.RealizationMemento realization =
                RealizationTags.tagFirstClass("concept:add", "${x} + ${y}", "binary-operator");

        RealizationTags.FirstClass firstClass =
                assertInstanceOf(RealizationTags.FirstClass.class, realization);
        assertEquals("${x} + ${y}", firstClass.syntacticPattern());
        assertEquals("binary-operator", firstClass.surfaceLocator());
    }

    @Test
    void tagCompositionBuildsCompositionRealization() {
        RealizationTags.RealizationMemento realization =
                RealizationTags.tagComposition("concept:list", COMPOSITION_TREE_CID);

        RealizationTags.Composition composition =
                assertInstanceOf(RealizationTags.Composition.class, realization);
        assertEquals(COMPOSITION_TREE_CID, composition.compositionTreeCid());
    }

    @Test
    void tagBoundaryBuildsBoundaryRealization() {
        RealizationTags.RealizationMemento realization = RealizationTags.tagBoundary(
                "concept:http-request",
                "python-requests",
                "requests.get",
                BOUNDARY_CONTRACT_CID);

        RealizationTags.Boundary boundary =
                assertInstanceOf(RealizationTags.Boundary.class, realization);
        assertEquals("python-requests", boundary.library());
        assertEquals("requests.get", boundary.api());
        assertEquals(BOUNDARY_CONTRACT_CID, boundary.boundaryContractCid());
    }

    @Test
    void tagSugarCarrierBuildsSugarCarrierRealization() {
        RealizationTags.RealizationMemento realization =
                RealizationTags.tagSugarCarrier("concept:free");

        assertInstanceOf(RealizationTags.SugarCarrier.class, realization);
    }

    @Test
    void taggingPrimitivesHaveDistinctCids() {
        Set<String> cids = Set.of(
                RealizationTags.tagFirstClass("concept:add", "${x} + ${y}", "binary-operator")
                        .recomputeCid(),
                RealizationTags.tagComposition("concept:add", COMPOSITION_TREE_CID).recomputeCid(),
                RealizationTags.tagBoundary(
                                "concept:add",
                                "python-requests",
                                "requests.get",
                                BOUNDARY_CONTRACT_CID)
                        .recomputeCid(),
                RealizationTags.tagSugarCarrier("concept:add").recomputeCid());

        assertEquals(4, cids.size());
    }
}
