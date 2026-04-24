import { registerArtifactKind } from "../artifactKindRegistry.js";
import { primaryFixFiles, touchesAny, isConfigFile } from "./helpers.js";

export function registerConfigUpdate() {
  registerArtifactKind({
    name: "config_update",
    description: "Env var, feature flag, or deployment setting change.",
    oraclesThatApply: [10, 11],
    isPresent: (a) => touchesAny(primaryFixFiles(a), isConfigFile),
    bundleTypeScope: "both",
  });
}
registerConfigUpdate();
