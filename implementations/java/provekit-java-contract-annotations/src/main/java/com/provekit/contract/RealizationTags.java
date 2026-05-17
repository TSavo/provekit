package com.provekit.contract;

import com.provekit.ir.Jcs;
import java.util.Objects;

/** Builder-style tagging primitives for per-concept realization metadata. */
public final class RealizationTags {
    private RealizationTags() {}

    /** A tagged realization memento matching the substrate enum shape. */
    public sealed interface RealizationMemento
            permits FirstClass, Composition, Boundary, SugarCarrier {
        Jcs.Json toJson();

        default String toJcsString() {
            return Jcs.encode(toJson());
        }

        default String recomputeCid() {
            return Jcs.cid(toJson());
        }
    }

    /** A first-class language-native concept realization. */
    public record FirstClass(String syntacticPattern, String surfaceLocator)
            implements RealizationMemento {
        public FirstClass {
            syntacticPattern = requireText(syntacticPattern, "syntacticPattern");
            surfaceLocator = requireText(surfaceLocator, "surfaceLocator");
        }

        @Override
        public Jcs.Json toJson() {
            return Jcs.object(
                    "kind", Jcs.string("first-class"),
                    "surface_locator", Jcs.string(surfaceLocator),
                    "syntactic_pattern", Jcs.string(syntacticPattern));
        }
    }

    /** A realization expressed as a content-addressed concept composition tree. */
    public record Composition(String compositionTreeCid) implements RealizationMemento {
        public Composition {
            compositionTreeCid = requireText(compositionTreeCid, "compositionTreeCid");
        }

        @Override
        public Jcs.Json toJson() {
            return Jcs.object(
                    "composition_tree_cid", Jcs.string(compositionTreeCid),
                    "kind", Jcs.string("composition"));
        }
    }

    /** A library or API boundary realization. */
    public record Boundary(String library, String api, String boundaryContractCid)
            implements RealizationMemento {
        public Boundary {
            library = requireText(library, "library");
            api = requireText(api, "api");
            boundaryContractCid = requireText(boundaryContractCid, "boundaryContractCid");
        }

        @Override
        public Jcs.Json toJson() {
            return Jcs.object(
                    "api", Jcs.string(api),
                    "boundary_contract_cid", Jcs.string(boundaryContractCid),
                    "kind", Jcs.string("boundary"),
                    "library", Jcs.string(library));
        }
    }

    /** A realization carried implicitly by concept-citation comment sugar. */
    public record SugarCarrier() implements RealizationMemento {
        @Override
        public Jcs.Json toJson() {
            return Jcs.object("kind", Jcs.string("sugar-carrier"));
        }
    }

    /**
     * Tag a concept op with a language-native syntactic form.
     *
     * <pre>{@code
     * RealizationTags.RealizationMemento realization =
     *         RealizationTags.tagFirstClass("concept:add", "${x} + ${y}", "binary-operator");
     * // Returns RealizationTags.FirstClass
     * }</pre>
     */
    public static RealizationMemento tagFirstClass(
            String conceptOp, String syntacticPattern, String surfaceLocator) {
        requireText(conceptOp, "conceptOp");
        return new FirstClass(syntacticPattern, surfaceLocator);
    }

    /**
     * Tag a concept op with a content-addressed composition tree.
     *
     * <pre>{@code
     * RealizationTags.RealizationMemento realization =
     *         RealizationTags.tagComposition("concept:list", compositionTreeCid);
     * // Returns RealizationTags.Composition
     * }</pre>
     */
    public static RealizationMemento tagComposition(String conceptOp, String compositionTree) {
        requireText(conceptOp, "conceptOp");
        return new Composition(compositionTree);
    }

    /**
     * Tag a concept op with a library or API boundary contract.
     *
     * <pre>{@code
     * RealizationTags.RealizationMemento realization =
     *         RealizationTags.tagBoundary(
     *                 "concept:http-request",
     *                 "python-requests",
     *                 "requests.get",
     *                 boundaryContractCid);
     * // Returns RealizationTags.Boundary
     * }</pre>
     */
    public static RealizationMemento tagBoundary(
            String conceptOp, String library, String api, String boundaryContractCid) {
        requireText(conceptOp, "conceptOp");
        return new Boundary(library, api, boundaryContractCid);
    }

    /**
     * Tag a concept op as a concept-citation sugar carrier.
     *
     * <pre>{@code
     * RealizationTags.RealizationMemento realization =
     *         RealizationTags.tagSugarCarrier("concept:free");
     * // Returns RealizationTags.SugarCarrier
     * }</pre>
     */
    public static RealizationMemento tagSugarCarrier(String conceptOp) {
        requireText(conceptOp, "conceptOp");
        return new SugarCarrier();
    }

    private static String requireText(String value, String field) {
        String text = Objects.requireNonNull(value, field);
        if (text.isEmpty()) {
            throw new IllegalArgumentException(field + " must be non-empty");
        }
        return text;
    }
}
