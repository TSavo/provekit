/**
 * B3: "data_ingress" remediation layer.
 *
 * Bug caused by unvalidated or malformed external input reaching the code.
 */

import { registerRemediationLayer } from "../remediationLayerRegistry.js";

const descriptor = {
  name: "data_ingress",
  description:
    "Bug caused by unvalidated or malformed external input reaching the code.",
  promptHint:
    `External data crosses a boundary (HTTP request, webhook, file upload, third-party API response) in an unexpected shape. Fix is schema validation at the boundary plus an error handler for rejected input. Examples: "NaN in a numeric field breaks downstream math", "missing optional field in third-party webhook body", "UTF-8 BOM in CSV upload".`,
  artifactKinds: ["schema_validation", "error_handler"],
};

export function registerDataIngressLayer(): void {
  registerRemediationLayer(descriptor);
}

// Self-register at module load.
registerDataIngressLayer();
