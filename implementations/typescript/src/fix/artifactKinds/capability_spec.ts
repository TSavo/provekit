import { registerArtifactKind } from "../artifactKindRegistry.js";

export function registerCapabilitySpec() {
  registerArtifactKind({
    name: "capability_spec",
    description: "Specification for a new SAST capability required by a substrate-extension bundle.",
    oraclesThatApply: [14, 15, 16, 17, 18],
    isPresent: (a) => a.capabilitySpec !== null,
    bundleTypeScope: "substrate",
  });
}
registerCapabilitySpec();
