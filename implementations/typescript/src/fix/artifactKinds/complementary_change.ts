import { registerArtifactKind } from "../artifactKindRegistry.js";

export function registerComplementaryChange() {
  registerArtifactKind({
    name: "complementary_change",
    description: "Adjacent site fixes, caller updates, or data-flow guards accompanying the primary fix.",
    oraclesThatApply: [3, 4, 5],
    isPresent: (a) => a.complementary.length > 0,
    bundleTypeScope: "both",
  });
}
registerComplementaryChange();
