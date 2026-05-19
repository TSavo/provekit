// SPDX-License-Identifier: Apache-2.0
//
// Fixture test for provekit.plugin.platform_semantics RPC handler.
//
// Golden CIDs are verified against the Rust reference implementation
// (provekit-ir-types DimensionValueMemento::recompute_cid +
//  PlatformSemanticTag::recompute_cid, KIT_ID="provekit-realize-java-core@0.1.0").

package com.provekit.realize;

import com.provekit.ir.Jcs;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

public class RpcServerPlatformSemanticsTest {

    // Canonical concept:literal CID (from #1282)
    private static final String CONCEPT_LITERAL_CID =
        "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6";

    // Golden dimension value CIDs (from Rust reference)
    private static final String CID_ARITHMETIC_OVERFLOW_WRAPPING =
        "blake3-512:a4aed351e390af2ca6b1076a8497aaba23717e09ebbb6354f30dc4c215d3250d50a35282f43eb520ed3dc8358541cb3edc33d728a10bdeff93fe1a05ba136569";
    private static final String CID_INTEGER_DIVISION_ROUNDING_TRUNCATE =
        "blake3-512:0bd40f590e54cda96d3da54526b027f40c2380f13840de5746aad47df54416dd0fe8949f4317a2363aa15032a22726b86896bf8a4d7cc7664cfd9cda726b78c9";
    private static final String CID_SHIFT_MODE_ARITHMETIC =
        "blake3-512:e6b118f0f19f0878db021c8332457adba51c6fe28943a306c88e168b8b69e724fd076a98a906e0cad7afa6e310543920b2efb9805c3eae74911c0cf534c30253";
    private static final String CID_SHIFT_MODE_LOGICAL =
        "blake3-512:d69d99f8502b0ee7dbbfe5a288de373a6f1a3f5fc2476f0ff88ea10e64947f90770d7cc673c8aec212fe4fd26672fa17004f6637def003b1eececae43ce1e95b";
    private static final String CID_NULL_SEMANTICS_THROW_ARITHMETIC_EXCEPTION =
        "blake3-512:8c8ec297d6611af9634fe208bbcbae4ea65e12ebbdb290fa0aa044b0f3f21c796b674f0f113245c8cb4a39fcff2a2100c7435292b5fdf23eba6a2f34d4ead5cc";
    private static final String CID_BITWISE_SEMANTICS_TWOS_COMPLEMENT =
        "blake3-512:222818aea5033dc484fb3fa06d1c28a9f8693f7ff6cabcb73cee93ca6cff2598de137a4942203073e3ad6199a6609ebd149eb4d70c102a15c33a6a989c447e52";
    // concept:literal SortAdmission: Java admits full primitive tier (Int, Float, String, Bool, Bytes, Null).
    // DV CID matches Python ("FullPrimitiveTier", same admission set, kit_cid elided).
    private static final String CID_SORT_ADMISSION_FULL_PRIMITIVE_TIER =
        "blake3-512:cdec58a9736ce0d4efc81a73cde61e1776df742a67de9141e49cf6712ae580f948dc3ee7bfa136e7904cb6d579e50f9644845fe50f8d4ddfeb68902562f13f85";

    // Golden tag CIDs (from Rust reference)
    private static final String CID_TAG_ADD =
        "blake3-512:a51bc88e7819574c965522eaa2c4c18efc7e793e2a94e2c8eb23a9c7d0eb1487a2aa01a744d31157dcfa833b7720dfafe58dc02f5eca6e89a7b716e8889260af";
    private static final String CID_TAG_SUB =
        "blake3-512:c604a9489bafbfd60f06574adcb5e2351bbd29814d15157f7bb9502e233a770d545ec1b961855c4b2cd714686502960aae0dd54370f9ce9e0afdd7743a4e6065";
    private static final String CID_TAG_MUL =
        "blake3-512:b74c57dcaea98be23e04380c0f6a3940081e8f880602ead2a439449dea789d5fb808117b5e51cf52d6040d12edb26eb75b7f4cbbe191acb25abebe3915f2f818";
    private static final String CID_TAG_NEG =
        "blake3-512:8c6f6563b4bf636610d8772591636c2ebd3efe17481378e40d48b61afc598eae61d5aa9ce49e4c48a79c9dba2ffc51d0625bfa06f03de7e9a51cd66c7d5da72e";
    private static final String CID_TAG_DIV =
        "blake3-512:ab24257a81695821f3c68ee07c43554ad3b8c474302820f3a97aa029678eb727c2cf0f11dabf4e7ab423c3820da10a71e0300512c0d51fd2ddcaca17a501d55b";
    private static final String CID_TAG_MOD =
        "blake3-512:f6b292560aa19abc706fdcb80a0c3f6bc3555bd1eaba8895e07d5ee511aee97a9d2edfff1f751bd9e5b6322929cede4fb09a3265765aa9813e026ae6f1465667";
    private static final String CID_TAG_SHL =
        "blake3-512:87964ec9b6fd00c6881b2c164d6ba4c54e1cd92ed5c4ef474dc544afeda300d4e91927b76b51fb04af0c8640ebd122720410e472b7b945cf6a4d4842ba84f682";
    private static final String CID_TAG_SHR =
        "blake3-512:9203d2069cf8220e3b579e75c3f932930da0e9e60de8d3b8e1149da8a9550a665b05dd8cb53c587e94dbe1905fbbe1b07fc9474d7383a52bb2ce90a559ee072a";
    private static final String CID_TAG_USHR =
        "blake3-512:7693cb3b0eee68235f4845bb13408ee20847246f385608de1d0260bca6f23ad8974be005619db8228b2a2dc375fede9a554dee9df2b7929b5ceeadb6f2882228";
    private static final String CID_TAG_BITNOT =
        "blake3-512:6e2dd52738fdf5f8f6d78e7fa4c4f817b086e6946dfc3f4ddc8eb4540ae4bab7abb7361f5763951c30cadbf46bfd4aa720d24337351f778ea8b05369d3444cd6";
    // concept:literal tag CID: matches Python (same DV CID, kit_cid elided, same op_cid)
    private static final String CID_TAG_LITERAL =
        "blake3-512:5523f7d24d51ff0a0d8ac96798946e4bc1722a7d1936918f4b9477b2246bde112d24f8043680fd22a5272e39d5bedbb2acc2d17590f6ff7afae140dd523c361d";

    @Test
    public void toJsonIsValidJson() {
        String json = PlatformSemanticsDeclaration.toJson();
        assertNotNull(json);
        assertFalse(json.isBlank());
        // Must parse without exception
        Jcs.Json parsed = Jcs.parse(json);
        assertNotNull(parsed);
    }

    @Test
    public void toJsonContainsElevenTags() {
        Jcs.Json parsed = Jcs.parse(PlatformSemanticsDeclaration.toJson());
        Jcs.Arr tags = ((Jcs.Obj) parsed).arrayField("tags");
        assertEquals(11, tags.values().size(), "expected 11 platform semantic tags (10 operators + concept:literal)");
    }

    @Test
    public void toJsonContainsSevenDimensionValues() {
        Jcs.Json parsed = Jcs.parse(PlatformSemanticsDeclaration.toJson());
        Jcs.Arr dimVals = ((Jcs.Obj) parsed).arrayField("dimension_values");
        assertEquals(7, dimVals.values().size(), "expected 7 dimension values (6 original + SortAdmission)");
    }

    @Test
    public void dimensionValueCidsMatchGolden() {
        Jcs.Json parsed = Jcs.parse(PlatformSemanticsDeclaration.toJson());
        Jcs.Arr dimVals = ((Jcs.Obj) parsed).arrayField("dimension_values");

        record GoldenDimValue(String dimensionName, String valueName, String expectedCid) {}
        List<GoldenDimValue> goldens = List.of(
            new GoldenDimValue("ArithmeticOverflow",      "Wrapping",                 CID_ARITHMETIC_OVERFLOW_WRAPPING),
            new GoldenDimValue("IntegerDivisionRounding", "Truncate",                 CID_INTEGER_DIVISION_ROUNDING_TRUNCATE),
            new GoldenDimValue("ShiftMode",               "Arithmetic",               CID_SHIFT_MODE_ARITHMETIC),
            new GoldenDimValue("ShiftMode",               "Logical",                  CID_SHIFT_MODE_LOGICAL),
            new GoldenDimValue("NullSemantics",           "ThrowArithmeticException", CID_NULL_SEMANTICS_THROW_ARITHMETIC_EXCEPTION),
            new GoldenDimValue("BitwiseSemantics",        "TwosComplement",           CID_BITWISE_SEMANTICS_TWOS_COMPLEMENT),
            new GoldenDimValue("SortAdmission",           "FullPrimitiveTier",        CID_SORT_ADMISSION_FULL_PRIMITIVE_TIER)
        );

        assertEquals(goldens.size(), dimVals.values().size());
        for (int i = 0; i < goldens.size(); i++) {
            GoldenDimValue g = goldens.get(i);
            Jcs.Obj obj = (Jcs.Obj) dimVals.get(i);
            assertEquals(g.dimensionName(), obj.stringField("dimension_name"),
                "dimension_name at index " + i);
            assertEquals(g.valueName(), obj.stringField("value_name"),
                "value_name at index " + i);
            assertEquals(g.expectedCid(), obj.stringField("cid"),
                "cid at index " + i + " (" + g.dimensionName() + "/" + g.valueName() + ")");
        }
    }

    @Test
    public void tagCidsMatchGoldenByOpCid() {
        Jcs.Json parsed = Jcs.parse(PlatformSemanticsDeclaration.toJson());
        Jcs.Arr tags = ((Jcs.Obj) parsed).arrayField("tags");

        // Concept op CIDs from the Java kit data
        String addOpCid    = "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468";
        String subOpCid    = "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af";
        String mulOpCid    = "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03";
        String negOpCid    = "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409";
        String divOpCid    = "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839";
        String modOpCid    = "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d";
        String shlOpCid    = "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a";
        String shrOpCid    = "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b";
        String ushrOpCid   = "blake3-512:5746cb4f8bb8d713624731661de51e851e7ca65dae10a88bae4727d1e0070525be77e9919d90939264acaf4c093b00808862e6d0d2c24ac05262ce95cd67c8ad";
        String bitnotOpCid = "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f";

        String literalOpCid = CONCEPT_LITERAL_CID;

        record GoldenTag(String opCid, String expectedCid) {}
        List<GoldenTag> goldens = List.of(
            new GoldenTag(addOpCid,    CID_TAG_ADD),
            new GoldenTag(subOpCid,    CID_TAG_SUB),
            new GoldenTag(mulOpCid,    CID_TAG_MUL),
            new GoldenTag(negOpCid,    CID_TAG_NEG),
            new GoldenTag(divOpCid,    CID_TAG_DIV),
            new GoldenTag(modOpCid,    CID_TAG_MOD),
            new GoldenTag(shlOpCid,    CID_TAG_SHL),
            new GoldenTag(shrOpCid,    CID_TAG_SHR),
            new GoldenTag(ushrOpCid,   CID_TAG_USHR),
            new GoldenTag(bitnotOpCid, CID_TAG_BITNOT),
            new GoldenTag(literalOpCid, CID_TAG_LITERAL)
        );

        assertEquals(goldens.size(), tags.values().size());
        for (int i = 0; i < goldens.size(); i++) {
            GoldenTag g = goldens.get(i);
            Jcs.Obj obj = (Jcs.Obj) tags.get(i);
            assertEquals(g.opCid(), obj.stringField("op_cid"),
                "op_cid at index " + i);
            assertEquals(g.expectedCid(), obj.stringField("cid"),
                "cid at index " + i + " (op=" + g.opCid().substring(0, 20) + "...)");
        }
    }

    @Test
    public void noOpAliasesField() {
        // op_aliases is empty, serde skip_serializing_if means it should not appear in output
        // The Java JSON output omits it (empty map = not emitted)
        String json = PlatformSemanticsDeclaration.toJson();
        assertFalse(json.contains("op_aliases"),
            "op_aliases must not appear when empty");
    }

    @Test
    public void compareToIrFormulaAtomicShape() {
        // Operator dimension values: compare_to is {args:[], kind:"atomic", name:"java:X"}
        // SortAdmission: compare_to is {args:[...], kind:"atomic", name:"admits_sorts"}
        Jcs.Json parsed = Jcs.parse(PlatformSemanticsDeclaration.toJson());
        Jcs.Arr dimVals = ((Jcs.Obj) parsed).arrayField("dimension_values");
        for (Jcs.Json val : dimVals.values()) {
            Jcs.Obj obj = (Jcs.Obj) val;
            String dimensionName = obj.stringField("dimension_name");
            Jcs.Obj compareTo = obj.objectField("compare_to");
            assertEquals("atomic", compareTo.stringField("kind"),
                "compare_to.kind must be 'atomic' for " + dimensionName);
            String name = compareTo.stringField("name");
            if ("SortAdmission".equals(dimensionName)) {
                assertEquals("admits_sorts", name,
                    "SortAdmission compare_to.name must be 'admits_sorts'");
                Jcs.Arr args = compareTo.arrayField("args");
                assertFalse(args.isEmpty(), "SortAdmission compare_to.args must not be empty");
            } else {
                Jcs.Arr args = compareTo.arrayField("args");
                assertTrue(args.isEmpty(), "compare_to.args must be empty for " + dimensionName);
                assertTrue(name.startsWith("java:"),
                    "compare_to.name must start with 'java:' but was: " + name);
            }
        }
    }

    @Test
    public void conceptLiteralTagHasSortAdmissionOnly() {
        Jcs.Json parsed = Jcs.parse(PlatformSemanticsDeclaration.toJson());
        Jcs.Arr tags = ((Jcs.Obj) parsed).arrayField("tags");
        Jcs.Obj literalTag = null;
        for (Jcs.Json t : tags.values()) {
            Jcs.Obj tagObj = (Jcs.Obj) t;
            if (CONCEPT_LITERAL_CID.equals(tagObj.stringField("op_cid"))) {
                literalTag = tagObj;
                break;
            }
        }
        assertNotNull(literalTag, "concept:literal tag must be present");
        Jcs.Obj dims = literalTag.objectField("dimensions");
        // Must have exactly one dimension: SortAdmission
        assertEquals(CID_SORT_ADMISSION_FULL_PRIMITIVE_TIER,
            dims.stringField("SortAdmission"),
            "concept:literal SortAdmission must equal golden CID");
        assertEquals(CID_TAG_LITERAL, literalTag.stringField("cid"),
            "concept:literal tag CID must equal golden CID");
    }

    @Test
    public void toJsonIsStable() {
        // Must be deterministic across multiple calls
        assertEquals(PlatformSemanticsDeclaration.toJson(),
                     PlatformSemanticsDeclaration.toJson());
    }
}
