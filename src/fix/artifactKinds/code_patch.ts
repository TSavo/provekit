import { registerArtifactKind } from "../artifactKindRegistry.js";

export function registerCodePatch() {
  registerArtifactKind({
    name: "code_patch",
    description: "Source edit that fixes the primary bug site.",
    oraclesThatApply: [1, 2, 4, 5, 7, 8, 10, 11],
    isPresent: (a) => a.primaryFix !== null,
    bundleTypeScope: "both",
  });
}
registerCodePatch();
