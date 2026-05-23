package com.provekit.runtime;

import java.util.Objects;

/**
 * Substrate-runtime carrier for source-language sum-type variants
 * (rust enum variants, ML algebraic data types, etc.) where the lowered
 * java doesn't have a per-family sealed hierarchy to express the
 * variant.
 *
 * <p>The substrate concept is {@code concept:sum-variant-construct}.
 * Each instance carries:
 * <ul>
 *   <li>{@code family}: canonical sum-type family identifier
 *       (e.g. {@code "LiftError"})</li>
 *   <li>{@code variant}: variant name within the family
 *       (e.g. {@code "Internal"})</li>
 *   <li>{@code payload}: variant's payload (may be null for
 *       unit variants)</li>
 * </ul>
 *
 * <p>Callers do variant dispatch via {@link #isVariant(String)} +
 * {@link #payload()}. This preserves (B) functional opacity — the
 * dispatch information is preserved at runtime even though java's
 * static type system can't carry the per-family hierarchy without
 * mint-time inventory.
 *
 * <p>For families where the substrate has done the variant inventory
 * upfront, prefer a per-family sealed interface (rust-style enum
 * realization). This generic carrier is the fallback for ad-hoc
 * variants the substrate hasn't catalogued.
 */
public record SumVariant(String family, String variant, Object payload) {

    public SumVariant {
        Objects.requireNonNull(family, "family");
        Objects.requireNonNull(variant, "variant");
        // payload may be null (unit variants).
    }

    /** True iff this is the named variant of any family. Useful when the
     *  caller already knows the family. */
    public boolean isVariant(String variantName) {
        return variant.equals(variantName);
    }

    /** True iff this is the named (family, variant) pair. */
    public boolean is(String familyName, String variantName) {
        return family.equals(familyName) && variant.equals(variantName);
    }

    @Override
    public String toString() {
        if (payload == null) {
            return family + "::" + variant;
        }
        return family + "::" + variant + "(" + payload + ")";
    }
}
