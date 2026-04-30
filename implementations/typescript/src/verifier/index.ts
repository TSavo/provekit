/**
 * Unified verifier — protocol-first surface.
 *
 * The legacy per-invariant Z3 verifier (verifyAllCached) and its
 * ValidityReport shape have been removed; bridge enforcement is the
 * verifier now. Every consumer (CLI, LSP, CI gate) calls
 * runBridgeEnforcement and renders the resulting BridgeEnforcementReport.
 *
 * Spec: protocol/specs/2026-04-30-proof-file-format.md
 *       protocol/specs/2026-04-30-chain-validity-and-fail-closed.md
 */

export {
  runBridgeEnforcement,
  formatBridgeEnforcementReport,
} from "./bridgeEnforcement.js";

export type { BridgeEnforcementReport } from "./bridgeEnforcement.js";
export type { BridgeReportRow } from "../workflow/producers/reportBridgeViolations.js";
