import { registerArtifactKind } from "../artifactKindRegistry.js";

export function registerPrincipleCandidate() {
  registerArtifactKind({
    name: "principle_candidate",
    description: "A generalized principle derived from the bug fix.",
    oraclesThatApply: [6, 12, 13],
    isPresent: (a) => a.principle !== null,
    bundleTypeScope: "both",
  });
}
registerPrincipleCandidate();
