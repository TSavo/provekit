import { registerArtifactKind } from "../artifactKindRegistry.js";
import { primaryFixFiles, touchesAny, isLintFile } from "./helpers.js";

export function registerLintRule() {
  registerArtifactKind({
    name: "lint_rule",
    description: "ESLint/biome/custom lint rule addition that catches the bug pattern.",
    oraclesThatApply: [16],
    isPresent: (a) => touchesAny(primaryFixFiles(a), isLintFile),
    bundleTypeScope: "both",
  });
}
registerLintRule();
