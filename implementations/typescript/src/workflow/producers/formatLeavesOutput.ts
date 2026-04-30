/**
 * format-leaves-output Stage — render a leaves projection as text or JSON.
 *
 * Pure formatter; no DB dependency. Takes the projection from
 * enumerate-local-leaves and stringifies for human consumption (`text`,
 * default) or downstream tooling (`json`). Other formats are intentionally
 * not supported — proliferating output shapes is a maintenance tax we don't
 * want for a CLI surface.
 */

import type { Stage } from "../types.js";
import type { LocalLeaf } from "./enumerateLocalLeaves.js";

export const FORMAT_LEAVES_OUTPUT_CAPABILITY = "format-leaves-output";

export type LeavesOutputFormat = "text" | "json";

export interface FormatLeavesOutputStageInput {
  leaves: LocalLeaf[];
  format?: string | null;
}

export interface FormatLeavesOutputResult {
  format: LeavesOutputFormat;
  body: string;
}

export function makeFormatLeavesOutputStage(opts: {
  producerVersion?: string;
} = {}): Stage<FormatLeavesOutputStageInput, FormatLeavesOutputResult> {
  const producedBy = opts.producerVersion ?? "format-leaves-output@v1";

  return {
    name: "format-leaves-output",
    producedBy,

    serializeInput(input) {
      return {
        leaves: input.leaves,
        format: normalizeFormat(input.format),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as FormatLeavesOutputResult;
    },

    async run(input) {
      const format = normalizeFormat(input.format);
      const body =
        format === "json" ? renderJson(input.leaves) : renderText(input.leaves);
      return { format, body };
    },
  };
}

function normalizeFormat(value: string | null | undefined): LeavesOutputFormat {
  if (value === "json") return "json";
  return "text";
}

function renderJson(leaves: LocalLeaf[]): string {
  return JSON.stringify({ leaves }, null, 2);
}

function renderText(leaves: LocalLeaf[]): string {
  if (leaves.length === 0) {
    return "No locally-minted mementos.";
  }
  const lines: string[] = [];
  lines.push(`Locally-minted leaves: ${leaves.length}`);
  for (const leaf of leaves) {
    const kind = leaf.evidenceKind ?? "untyped";
    lines.push(
      `  ${leaf.cid} verdict=${leaf.verdict} producedBy=${leaf.producedBy} kind=${kind} property=${leaf.propertyHash}`,
    );
  }
  return lines.join("\n");
}
