import { registerArtifactKind } from "../artifactKindRegistry.js";
import { primaryFixFiles, touchesOnly, isTestFile } from "./helpers.js";

export function registerTestFix() {
  registerArtifactKind({
    name: "test_fix",
    description: "The test was wrong; primary fix touches only test files (no source changes).",
    oraclesThatApply: [9, 10],
    isPresent: (a) => touchesOnly(primaryFixFiles(a), isTestFile),
    bundleTypeScope: "both",
  });
}
registerTestFix();
