import { registerArtifactKind } from "../artifactKindRegistry.js";
import { primaryFixFiles, touchesAny, isPromptFile } from "./helpers.js";

export function registerPromptUpdate() {
  registerArtifactKind({
    name: "prompt_update",
    description: "LLM prompt change (for LLM-in-loop systems).",
    oraclesThatApply: [6],
    isPresent: (a) => touchesAny(primaryFixFiles(a), isPromptFile),
    bundleTypeScope: "both",
  });
}
registerPromptUpdate();
