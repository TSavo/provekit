/**
 * format-roots-output Stage — render a roots projection as text or JSON.
 *
 * Pure formatter; no DB dependency. Takes the external-CID list from
 * enumerate-local-roots and stringifies for an auditor's consumption
 * (`text`, default) or for tooling (`json`).
 *
 * Per docs/specs/2026-04-29-correctness-is-a-hash.md §"Naming discipline:
 * leaves AND roots, not walks", the framework's job here is to surface
 * the precise list of external CIDs an auditor must walk; the auditor
 * does the walking.
 */

import type { Stage } from "../types.js";

export const FORMAT_ROOTS_OUTPUT_CAPABILITY = "format-roots-output";

export type RootsOutputFormat = "text" | "json";

export interface FormatRootsOutputStageInput {
  roots: string[];
  format?: string | null;
}

export interface FormatRootsOutputResult {
  format: RootsOutputFormat;
  body: string;
}

export function makeFormatRootsOutputStage(opts: {
  producerVersion?: string;
} = {}): Stage<FormatRootsOutputStageInput, FormatRootsOutputResult> {
  const producedBy = opts.producerVersion ?? "format-roots-output@v1";

  return {
    name: "format-roots-output",
    producedBy,

    serializeInput(input) {
      return {
        roots: input.roots,
        format: normalizeFormat(input.format),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as FormatRootsOutputResult;
    },

    async run(input) {
      const format = normalizeFormat(input.format);
      const body =
        format === "json" ? renderJson(input.roots) : renderText(input.roots);
      return { format, body };
    },
  };
}

function normalizeFormat(value: string | null | undefined): RootsOutputFormat {
  if (value === "json") return "json";
  return "text";
}

function renderJson(roots: string[]): string {
  return JSON.stringify({ roots }, null, 2);
}

function renderText(roots: string[]): string {
  if (roots.length === 0) {
    return "No external roots — every referenced CID was minted locally.";
  }
  const lines: string[] = [];
  lines.push(`External roots: ${roots.length}`);
  for (const cid of roots) {
    lines.push(`  ${cid}`);
  }
  return lines.join("\n");
}
