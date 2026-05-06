package com.provekit.realize;

import java.util.List;

public record RealizerOutput(
    String kind,
    String schemaVersion,
    String mode,
    String status,
    String planCid,
    String gapCid,
    String patchCid,
    String transformedArtifactCid,
    String postLiftCid,
    String closureWitnessCid,
    String modifiedSource,
    String postLiftJson,
    List<String> diagnostics
) {
    public static RealizerOutput closed(
        RealizerPlan plan,
        String patchCid,
        String transformedArtifactCid,
        String postLiftCid,
        String closureWitnessCid,
        String modifiedSource,
        String postLiftJson
    ) {
        return new RealizerOutput(
            "RealizerOutput",
            "1",
            "transform",
            "closed",
            plan.planCid(),
            plan.gapCid(),
            patchCid,
            transformedArtifactCid,
            postLiftCid,
            closureWitnessCid,
            modifiedSource,
            postLiftJson,
            List.of()
        );
    }

    public static RealizerOutput refusal(RealizerPlan plan, String reason) {
        return new RealizerOutput(
            "RealizerOutput",
            "1",
            plan.mode(),
            "rejected",
            plan.planCid(),
            plan.gapCid(),
            null,
            null,
            null,
            null,
            plan.source(),
            "",
            List.of(reason)
        );
    }

    public static RealizerOutput candidate(RealizerPlan plan, String reason, String modifiedSource) {
        return new RealizerOutput(
            "RealizerOutput",
            "1",
            plan.mode(),
            "candidate",
            plan.planCid(),
            plan.gapCid(),
            null,
            null,
            null,
            null,
            modifiedSource,
            "",
            List.of(reason)
        );
    }

    public boolean hasClosedInvariantEvidence() {
        if (!"closed".equals(status)) return false;
        return nonEmpty(planCid)
            && nonEmpty(gapCid)
            && nonEmpty(patchCid)
            && nonEmpty(transformedArtifactCid)
            && nonEmpty(postLiftCid)
            && nonEmpty(closureWitnessCid)
            && nonEmpty(modifiedSource)
            && nonEmpty(postLiftJson);
    }

    public String toJson() {
        return "{"
            + "\"kind\":\"RealizerOutput\","
            + "\"schemaVersion\":\"1\","
            + "\"mode\":" + JsonUtil.quoted(mode) + ","
            + "\"status\":" + JsonUtil.quoted(status) + ","
            + "\"planCid\":" + nullable(planCid) + ","
            + "\"gapCid\":" + nullable(gapCid) + ","
            + "\"patchCid\":" + nullable(patchCid) + ","
            + "\"transformedArtifactCid\":" + nullable(transformedArtifactCid) + ","
            + "\"postLiftCid\":" + nullable(postLiftCid) + ","
            + "\"closureWitnessCid\":" + nullable(closureWitnessCid) + ","
            + "\"modifiedSource\":" + JsonUtil.quoted(modifiedSource) + ","
            + "\"postLift\":" + (postLiftJson == null || postLiftJson.isEmpty() ? "null" : postLiftJson) + ","
            + "\"diagnostics\":" + diagnosticsJson()
            + "}";
    }

    private String diagnosticsJson() {
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < diagnostics.size(); i++) {
            if (i > 0) sb.append(',');
            sb.append(JsonUtil.quoted(diagnostics.get(i)));
        }
        sb.append(']');
        return sb.toString();
    }

    private static String nullable(String s) {
        return s == null ? "null" : JsonUtil.quoted(s);
    }

    private static boolean nonEmpty(String s) {
        return s != null && !s.isEmpty();
    }
}
