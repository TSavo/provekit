// SPDX-License-Identifier: Apache-2.0
//
// Java kit platform semantics declaration.
//
// Implements the provekit.plugin.platform_semantics RPC method (PEP 1.7.0).
// Returns the JSON payload that libprovekit deserializes into a
// PlatformSemanticsDeclaration for the "java" target.
//
// CID computation follows the substrate spec:
//   DimensionValueMemento CID = blake3-512(JCS(memento WITHOUT cid + kit_cid))
//   PlatformSemanticTag CID   = blake3-512(JCS(tag WITHOUT cid + kit_cid))
//
// The golden CID values here are verified against the Rust reference
// implementation (provekit-ir-types DimensionValueMemento::recompute_cid +
// PlatformSemanticTag::recompute_cid).

package com.provekit.realize;

import com.provekit.ir.Blake3;
import com.provekit.ir.Jcs;
import com.provekit.ir.Jcs.Value;

import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;

/** Builds and caches the platform_semantics JSON response for the Java kit. */
public final class PlatformSemanticsDeclaration {
    private PlatformSemanticsDeclaration() {}

    private static final String KIT_ID = "provekit-realize-java-core@0.1.0";

    // concept:literal CID (from #1282)
    private static final String CONCEPT_LITERAL_CID =
        "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6";

    // Canonical sort CIDs (from #1282)
    // Java admits: Int, Float, String, Bool, Bytes, Null (full primitive tier)
    // Args sorted alphabetically by CID string value:
    //   BOOL (0ee1...) < INT (30ff...) < NULL (62f6...) < BYTES (7116...) < FLOAT (b979...) < STRING (be87...)
    private static final String SORT_BOOL_CID =
        "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
    private static final String SORT_INT_CID =
        "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
    private static final String SORT_NULL_CID =
        "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5";
    private static final String SORT_BYTES_CID =
        "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";
    private static final String SORT_FLOAT_CID =
        "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
    private static final String SORT_STRING_CID =
        "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";

    // Concept op CIDs (from provekit substrate hub)
    private static final String CONCEPT_ADD_CID =
        "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468";
    private static final String CONCEPT_SUB_CID =
        "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af";
    private static final String CONCEPT_MUL_CID =
        "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03";
    private static final String CONCEPT_NEG_CID =
        "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409";
    private static final String CONCEPT_DIV_CID =
        "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839";
    private static final String CONCEPT_MOD_CID =
        "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d";
    private static final String CONCEPT_SHL_CID =
        "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a";
    private static final String CONCEPT_SHR_CID =
        "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b";
    private static final String CONCEPT_USHR_CID =
        "blake3-512:5746cb4f8bb8d713624731661de51e851e7ca65dae10a88bae4727d1e0070525be77e9919d90939264acaf4c093b00808862e6d0d2c24ac05262ce95cd67c8ad";
    private static final String CONCEPT_BITNOT_CID =
        "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f";

    // Cached JSON response (built once on first call)
    private static volatile String cachedJson = null;

    /**
     * Returns the complete JSON string for the provekit.plugin.platform_semantics result.
     * Thread-safe via double-checked locking.
     */
    public static String toJson() {
        if (cachedJson == null) {
            synchronized (PlatformSemanticsDeclaration.class) {
                if (cachedJson == null) {
                    cachedJson = buildJson();
                }
            }
        }
        return cachedJson;
    }

    private static String buildJson() {
        String kitCid = Blake3.blake3_512(KIT_ID.getBytes(StandardCharsets.UTF_8));

        // Build dimension values
        DimValue wrapping = dimValue(kitCid, "ArithmeticOverflow", "Wrapping");
        DimValue truncate = dimValue(kitCid, "IntegerDivisionRounding", "Truncate");
        DimValue arithmetic = dimValue(kitCid, "ShiftMode", "Arithmetic");
        DimValue logical = dimValue(kitCid, "ShiftMode", "Logical");
        DimValue throwArithEx = dimValue(kitCid, "NullSemantics", "ThrowArithmeticException");
        DimValue twosComplement = dimValue(kitCid, "BitwiseSemantics", "TwosComplement");
        // concept:literal SortAdmission: Java admits full primitive tier (Int, Float, String, Bool, Bytes, Null).
        // value_name "FullPrimitiveTier" matches Python for cross-kit substrate uniformity.
        DimValue sortAdmission = dimValueAdmitsSorts(kitCid, "SortAdmission", "FullPrimitiveTier",
            SORT_BOOL_CID, SORT_INT_CID, SORT_NULL_CID, SORT_BYTES_CID, SORT_FLOAT_CID, SORT_STRING_CID);

        // Build tags
        TagEntry addTag = tag(kitCid, CONCEPT_ADD_CID,
            new String[]{"ArithmeticOverflow", wrapping.cid});
        TagEntry subTag = tag(kitCid, CONCEPT_SUB_CID,
            new String[]{"ArithmeticOverflow", wrapping.cid});
        TagEntry mulTag = tag(kitCid, CONCEPT_MUL_CID,
            new String[]{"ArithmeticOverflow", wrapping.cid});
        TagEntry negTag = tag(kitCid, CONCEPT_NEG_CID,
            new String[]{"ArithmeticOverflow", wrapping.cid});
        TagEntry divTag = tag(kitCid, CONCEPT_DIV_CID,
            new String[]{"IntegerDivisionRounding", truncate.cid},
            new String[]{"NullSemantics", throwArithEx.cid});
        TagEntry modTag = tag(kitCid, CONCEPT_MOD_CID,
            new String[]{"IntegerDivisionRounding", truncate.cid},
            new String[]{"NullSemantics", throwArithEx.cid});
        TagEntry shlTag = tag(kitCid, CONCEPT_SHL_CID,
            new String[]{"BitwiseSemantics", twosComplement.cid});
        TagEntry shrTag = tag(kitCid, CONCEPT_SHR_CID,
            new String[]{"BitwiseSemantics", twosComplement.cid},
            new String[]{"ShiftMode", arithmetic.cid});
        TagEntry ushrTag = tag(kitCid, CONCEPT_USHR_CID,
            new String[]{"BitwiseSemantics", twosComplement.cid},
            new String[]{"ShiftMode", logical.cid});
        TagEntry bitnotTag = tag(kitCid, CONCEPT_BITNOT_CID,
            new String[]{"BitwiseSemantics", twosComplement.cid});
        // concept:literal tag: carries only the SortAdmission dimension
        TagEntry literalTag = tag(kitCid, CONCEPT_LITERAL_CID,
            new String[]{"SortAdmission", sortAdmission.cid});

        StringBuilder sb = new StringBuilder();
        sb.append("{\"tags\":[");
        appendTag(sb, addTag);
        sb.append(','); appendTag(sb, subTag);
        sb.append(','); appendTag(sb, mulTag);
        sb.append(','); appendTag(sb, negTag);
        sb.append(','); appendTag(sb, divTag);
        sb.append(','); appendTag(sb, modTag);
        sb.append(','); appendTag(sb, shlTag);
        sb.append(','); appendTag(sb, shrTag);
        sb.append(','); appendTag(sb, ushrTag);
        sb.append(','); appendTag(sb, bitnotTag);
        sb.append(','); appendTag(sb, literalTag);
        sb.append("],\"dimension_values\":[");
        appendDimValue(sb, wrapping);
        sb.append(','); appendDimValue(sb, truncate);
        sb.append(','); appendDimValue(sb, arithmetic);
        sb.append(','); appendDimValue(sb, logical);
        sb.append(','); appendDimValue(sb, throwArithEx);
        sb.append(','); appendDimValue(sb, twosComplement);
        sb.append(','); appendDimValue(sb, sortAdmission);
        sb.append("]}");
        return sb.toString();
    }

    // Internal records

    private record DimValue(
        String cid,
        String kitCid,
        String dimensionName,
        String valueName,
        String compareTo // JCS-encoded IrFormula::Atomic JSON string
    ) {}

    private record TagEntry(
        String cid,
        String kitCid,
        String opCid,
        LinkedHashMap<String, String> dimensions // sorted BTreeMap order (code-point)
    ) {}

    // Construct a DimensionValueMemento and compute its CID.
    // compare_to = IrFormula::Atomic{name:"java:{valueName}",args:[]}
    // JCS field order (code-point): args < kind < name
    // CID = blake3-512(JCS(without cid + kit_cid))
    //   = blake3-512(JCS({compare_to,dimension_name,kind,schemaVersion,value_name}))
    // JCS field order: compare_to < dimension_name < kind < schemaVersion < value_name
    private static DimValue dimValue(String kitCid, String dimensionName, String valueName) {
        // compare_to object (IrFormula::Atomic): JCS keys: args, kind, name
        Value compareToVal = Value.object(
            "args", Value.array(),
            "kind", Value.string("atomic"),
            "name", Value.string("java:" + valueName)
        );

        // Full memento without cid + kit_cid for CID computation
        // JCS key order: compare_to < dimension_name < kind < schemaVersion < value_name
        Value forCid = Value.object(
            "compare_to", compareToVal,
            "dimension_name", Value.string(dimensionName),
            "kind", Value.string("platform-dimension-value"),
            "schemaVersion", Value.string("1.0.0"),
            "value_name", Value.string(valueName)
        );

        String cid = Jcs.blake3Cid(forCid);
        return new DimValue(cid, kitCid, dimensionName, valueName, Jcs.encode(compareToVal));
    }

    // Construct a DimensionValueMemento for SortAdmission with an admits_sorts formula.
    // compare_to = IrFormula::Atomic{name:"admits_sorts", args:[IrTerm::Const{kind:"const",sort:{kind:"primitive",name:"cid"},value:"<cid>"},...]}
    // sortCids must already be sorted alphabetically by string value.
    // JCS key order in IrTerm::Const: kind < sort < value (alphabetical)
    // JCS key order in Sort::Primitive: kind < name (alphabetical)
    // JCS key order in IrFormula::Atomic: args < kind < name (alphabetical)
    private static DimValue dimValueAdmitsSorts(
            String kitCid, String dimensionName, String valueName, String... sortCids) {
        // Build args array
        Value[] argValues = new Value[sortCids.length];
        for (int i = 0; i < sortCids.length; i++) {
            Value sortVal = Value.object(
                "kind", Value.string("primitive"),
                "name", Value.string("cid")
            );
            argValues[i] = Value.object(
                "kind", Value.string("const"),
                "sort", sortVal,
                "value", Value.string(sortCids[i])
            );
        }
        Value compareToVal = Value.object(
            "args", Value.array(argValues),
            "kind", Value.string("atomic"),
            "name", Value.string("admits_sorts")
        );

        Value forCid = Value.object(
            "compare_to", compareToVal,
            "dimension_name", Value.string(dimensionName),
            "kind", Value.string("platform-dimension-value"),
            "schemaVersion", Value.string("1.0.0"),
            "value_name", Value.string(valueName)
        );

        String cid = Jcs.blake3Cid(forCid);
        return new DimValue(cid, kitCid, dimensionName, valueName, Jcs.encode(compareToVal));
    }

    // Construct a PlatformSemanticTag and compute its CID.
    // pairs: alternating dimension_name, value_cid strings (must be in sorted order)
    // CID = blake3-512(JCS(without cid + kit_cid))
    //   = blake3-512(JCS({dimensions,kind,op_cid,schemaVersion}))
    // JCS field order: dimensions < kind < op_cid < schemaVersion
    @SuppressWarnings("varargs")
    private static TagEntry tag(String kitCid, String opCid, String[]... dimensionPairs) {
        // dimensions object - keys must be sorted by code-point for JCS
        LinkedHashMap<String, String> dims = new LinkedHashMap<>();
        // Sort dimension keys (they come in already-sorted from caller)
        java.util.TreeMap<String, String> sorted = new java.util.TreeMap<>(
            java.util.Comparator.comparingInt(s -> s.codePointAt(0)));
        // Use unicode code-point comparison for full correctness
        sorted = new java.util.TreeMap<>((a, b) -> {
            int i = 0, j = 0;
            while (i < a.length() && j < b.length()) {
                int ca = a.codePointAt(i);
                int cb = b.codePointAt(j);
                if (ca != cb) return Integer.compare(ca, cb);
                i += Character.charCount(ca);
                j += Character.charCount(cb);
            }
            return Integer.compare(a.length() - i, b.length() - j);
        });
        for (String[] pair : dimensionPairs) {
            sorted.put(pair[0], pair[1]);
        }
        dims.putAll(sorted);

        // Build dimensions Value object (keys already in JCS order via TreeMap)
        Value[] dimKvs = new Value[dims.size() * 2];
        int idx = 0;
        // We need Object... for Value.object(), so rebuild
        Object[] dimObjKvs = new Object[dims.size() * 2];
        int oidx = 0;
        for (java.util.Map.Entry<String, String> e : dims.entrySet()) {
            dimObjKvs[oidx++] = e.getKey();
            dimObjKvs[oidx++] = Value.string(e.getValue());
        }
        Value dimensionsVal = Value.object(dimObjKvs);

        // Full tag without cid + kit_cid for CID computation
        // JCS key order: dimensions < kind < op_cid < schemaVersion
        Value forCid = Value.object(
            "dimensions", dimensionsVal,
            "kind", Value.string("platform-semantic-tag"),
            "op_cid", Value.string(opCid),
            "schemaVersion", Value.string("1.0.0")
        );

        String cid = Jcs.blake3Cid(forCid);
        return new TagEntry(cid, kitCid, opCid, dims);
    }

    private static void appendDimValue(StringBuilder sb, DimValue v) {
        // Wire JSON field order matches serde Rust struct field order:
        // cid, compare_to, dimension_name, kind, kit_cid, schemaVersion, value_name
        sb.append("{\"cid\":").append(JsonUtil.quoted(v.cid))
          .append(",\"compare_to\":").append(v.compareTo)
          .append(",\"dimension_name\":").append(JsonUtil.quoted(v.dimensionName))
          .append(",\"kind\":\"platform-dimension-value\"")
          .append(",\"kit_cid\":").append(JsonUtil.quoted(v.kitCid))
          .append(",\"schemaVersion\":\"1.0.0\"")
          .append(",\"value_name\":").append(JsonUtil.quoted(v.valueName))
          .append("}");
    }

    private static void appendTag(StringBuilder sb, TagEntry t) {
        // Wire JSON field order matches serde Rust struct field order:
        // cid, dimensions, kind, kit_cid, op_cid, schemaVersion
        sb.append("{\"cid\":").append(JsonUtil.quoted(t.cid))
          .append(",\"dimensions\":{");
        boolean first = true;
        for (java.util.Map.Entry<String, String> e : t.dimensions.entrySet()) {
            if (!first) sb.append(',');
            first = false;
            sb.append(JsonUtil.quoted(e.getKey())).append(':').append(JsonUtil.quoted(e.getValue()));
        }
        sb.append("}")
          .append(",\"kind\":\"platform-semantic-tag\"")
          .append(",\"kit_cid\":").append(JsonUtil.quoted(t.kitCid))
          .append(",\"op_cid\":").append(JsonUtil.quoted(t.opCid))
          .append(",\"schemaVersion\":\"1.0.0\"")
          .append("}");
    }
}
