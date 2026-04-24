import { registerArtifactKind } from "../artifactKindRegistry.js";

export function registerStartupAssert() {
  registerArtifactKind({
    name: "startup_assert",
    description: "Precondition guard injected at program startup.",
    oraclesThatApply: [11],
    isPresent: () => false,
    bundleTypeScope: "both",
  });
}
registerStartupAssert();
