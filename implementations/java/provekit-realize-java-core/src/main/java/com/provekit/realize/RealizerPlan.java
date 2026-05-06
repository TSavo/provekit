package com.provekit.realize;

import java.nio.charset.StandardCharsets;

import com.provekit.claimenvelope.Blake3;

public record RealizerPlan(
    String kind,
    String schemaVersion,
    String mode,
    String gapCid,
    String sourcePredicate,
    String targetPredicate,
    String policyCid,
    String surface,
    String targetSymbol,
    String proofVar,
    String source
) {
    public static RealizerPlan transform(
        String gapCid,
        String sourcePredicate,
        String targetPredicate,
        String policyCid,
        String surface,
        String targetSymbol,
        String proofVar,
        String source
    ) {
        return new RealizerPlan(
            "RealizerPlan",
            "1",
            "transform",
            gapCid,
            sourcePredicate,
            targetPredicate,
            policyCid,
            surface,
            targetSymbol,
            proofVar,
            source
        );
    }

    public static RealizerPlan fromJsonLine(String json) {
        String sourcePredicate = firstNonEmpty(
            JsonUtil.decodeJsonStringField(json, "sourcePredicate"),
            JsonUtil.decodeJsonStringField(json, "sourcePredicateCid")
        );
        String targetPredicate = firstNonEmpty(
            JsonUtil.decodeJsonStringField(json, "targetPredicate"),
            JsonUtil.decodeJsonStringField(json, "targetPredicateCid")
        );
        return transform(
            JsonUtil.decodeJsonStringField(json, "gapCid"),
            sourcePredicate,
            targetPredicate,
            JsonUtil.decodeJsonStringField(json, "policyCid"),
            JsonUtil.decodeJsonStringField(json, "surface"),
            JsonUtil.decodeJsonStringField(json, "targetSymbol"),
            JsonUtil.decodeJsonStringField(json, "proofVar"),
            JsonUtil.decodeJsonStringField(json, "source")
        );
    }

    public String planCid() {
        return Blake3.blake3_512(toJson().getBytes(StandardCharsets.UTF_8));
    }

    public String sourceArtifactCid() {
        return Blake3.blake3_512(source.getBytes(StandardCharsets.UTF_8));
    }

    public String toJson() {
        return "{"
            + "\"kind\":\"RealizerPlan\","
            + "\"schemaVersion\":\"1\","
            + "\"mode\":" + JsonUtil.quoted(mode) + ","
            + "\"gapCid\":" + JsonUtil.quoted(gapCid) + ","
            + "\"sourcePredicate\":" + JsonUtil.quoted(sourcePredicate) + ","
            + "\"targetPredicate\":" + JsonUtil.quoted(targetPredicate) + ","
            + "\"policyCid\":" + JsonUtil.quoted(policyCid) + ","
            + "\"surface\":" + JsonUtil.quoted(surface) + ","
            + "\"targetSymbol\":" + JsonUtil.quoted(targetSymbol) + ","
            + "\"proofVar\":" + JsonUtil.quoted(proofVar) + ","
            + "\"source\":" + JsonUtil.quoted(source)
            + "}";
    }

    private static String firstNonEmpty(String first, String second) {
        return first == null || first.isEmpty() ? second : first;
    }
}
