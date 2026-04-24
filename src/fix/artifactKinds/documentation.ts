import { registerArtifactKind } from "../artifactKindRegistry.js";

export function registerDocumentation() {
  registerArtifactKind({
    name: "documentation",
    description: "Documentation update accompanying a fix.",
    oraclesThatApply: [],
    isPresent: () => false,
    bundleTypeScope: "both",
  });
}
registerDocumentation();
