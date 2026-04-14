/**
 * Pipeline orchestrator.
 *
 * Five phases, immutable outputs, filesystem as the bus.
 * Each phase reads from disk, writes to disk.
 */

export { buildDependencyGraph, DependencyGraph } from "./phase1-dependencies";
export { assembleContexts, ContextBundle } from "./phase2-context";
export { deriveContracts, DerivationOutput } from "./phase3-derivation";
export { classifyPrinciples, PrincipleOutput } from "./phase4-principles";
export { applyAxiomsPhase, AxiomReport } from "./phase5-axioms";
