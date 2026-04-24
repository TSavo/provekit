import { registerArtifactKind } from "../artifactKindRegistry.js";
import { primaryFixFiles, touchesAny, isDepFile } from "./helpers.js";

export function registerDependencyUpdate() {
  registerArtifactKind({
    name: "dependency_update",
    description: "package.json or lockfile change.",
    oraclesThatApply: [10],
    isPresent: (a) => touchesAny(primaryFixFiles(a), isDepFile),
    bundleTypeScope: "both",
  });
}
registerDependencyUpdate();
