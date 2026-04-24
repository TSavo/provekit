import { registerArtifactKind } from "../artifactKindRegistry.js";

export function registerRegressionTest() {
  registerArtifactKind({
    name: "regression_test",
    description: "Test case that fails on the original code and passes on the fix.",
    oraclesThatApply: [9, 10],
    isPresent: (a) => a.test !== null,
    bundleTypeScope: "both",
  });
}
registerRegressionTest();
