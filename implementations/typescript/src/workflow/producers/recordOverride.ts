/**
 * record-override Stage — formalize a developer's intent to bypass the
 * provekit pre-commit gate.
 *
 * In the imperative `runOverride`, this command did nothing more than
 * print "Override recorded: <reason>" and remind the user to add
 * --no-verify. The migrated workflow lifts that into a content-addressable
 * claim: the override-record is a Stage output keyed at the workflow
 * binding so future audit walks can answer "did anyone override on this
 * codebase, and if so, when and why."
 *
 * Pure: no filesystem writes, no git mutations. The override is the
 * RECORD, not the side effect (the side effect is the developer running
 * `git commit --no-verify`, which is outside the workflow's scope).
 *
 * If a future iteration wants to persist the record to .provekit/overrides/
 * for cross-session audit, that becomes a separate Action — keep this
 * Stage pure.
 */

import type { Stage } from "../types.js";

export const RECORD_OVERRIDE_CAPABILITY = "record-override";

export interface RecordOverrideStageInput {
  /** Free-form justification the developer is committing to. */
  reason: string;
}

export interface RecordOverrideStageOutput {
  reason: string;
  /** Concrete CLI hint surfaced to the user. */
  followupCommand: string;
  /** Render-ready single-line summary for the dispatcher to print. */
  message: string;
}

export interface MakeRecordOverrideStageDeps {
  /** Override producer identity. Default: "record-override@v1". */
  producerVersion?: string;
}

export function makeRecordOverrideStage(
  deps: MakeRecordOverrideStageDeps = {},
): Stage<RecordOverrideStageInput, RecordOverrideStageOutput> {
  const producedBy = deps.producerVersion ?? "record-override@v1";

  return {
    name: "record-override",
    producedBy,

    serializeInput(input) {
      return { reason: input.reason };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as RecordOverrideStageOutput;
    },

    async run(input) {
      if (typeof input.reason !== "string" || input.reason.trim().length === 0) {
        throw new Error("record-override requires a non-empty reason");
      }
      return {
        reason: input.reason,
        followupCommand: "git commit --no-verify",
        message: `Override recorded: ${input.reason}`,
      };
    },
  };
}
