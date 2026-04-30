import { registerArtifactKind } from "../artifactKindRegistry.js";

export function registerObservabilityHook() {
  registerArtifactKind({
    name: "observability_hook",
    description: "Metrics or logging hook added at the fix site.",
    oraclesThatApply: [11],
    isPresent: (a) => a.complementary.some((c) => c.kind === "observability"),
    bundleTypeScope: "both",
  });
}
registerObservabilityHook();
