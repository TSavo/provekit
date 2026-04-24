import { registerArtifactKind } from "../artifactKindRegistry.js";
import { touchedFiles, touchesAny, isDocFile } from "./helpers.js";

export function registerDocumentation() {
  registerArtifactKind({
    name: "documentation",
    description: "Documentation update accompanying a fix.",
    oraclesThatApply: [],
    isPresent: (a) => touchesAny(touchedFiles(a), isDocFile),
    bundleTypeScope: "both",
  });
}
registerDocumentation();
