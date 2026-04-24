/**
 * B3: Remediation layer registry.
 *
 * The set of supported remediation layers is a runtime registry — not a
 * hardcoded enum. Adding a new category is a registerRemediationLayer() call
 * with no changes to classifier or type code.
 *
 * Mirrors the shape of src/fix/intakeRegistry.ts (B1 intake adapter registry).
 */

/**
 * A remediation layer is a category of fix. The LLM routes a BugSignal to
 * one primary layer and zero-or-more secondary layers. Each descriptor
 * ships the prompt fragment the classifier uses, the kinds of artifacts
 * the layer produces, and the examples that teach the LLM.
 */
export interface RemediationLayerDescriptor {
  /** Unique layer name. Appears in RemediationPlan.primaryLayer. */
  name: string;

  /** One-sentence description of what bugs belong in this layer. */
  description: string;

  /**
   * Longer prompt fragment — goes into the classifier's system prompt.
   * Should teach the LLM when to pick this layer. Include 1-2 concrete examples.
   * Newlines OK; keep under ~500 chars per layer to avoid bloating prompts.
   */
  promptHint: string;

  /**
   * Artifact kinds this layer can emit. When a bug is classified into a layer,
   * the artifacts listed here are the candidate output shapes. The orchestrator
   * (B5) picks which to actually produce. Strings match the artifact kind names
   * used downstream (code_patch, regression_test, startup_assert, etc.).
   *
   * Not a closed enum — this is just the layer's recommendation. Adding a new
   * artifact kind later in a different layer doesn't require changing this one.
   */
  artifactKinds: string[];

  /**
   * Whether this layer can participate in the substrate-extension path.
   * Only "code_invariant" should set this true for v1 — a code invariant the
   * DSL can't express triggers the capability-proposal path in C6.
   * Other layers (config, infrastructure, etc.) never trigger substrate bundles.
   */
  canTriggerSubstrateExtension?: boolean;
}

const registry = new Map<string, RemediationLayerDescriptor>();

/**
 * Register a remediation layer descriptor. Idempotent: duplicate names
 * overwrite (with a warning).
 */
export function registerRemediationLayer(d: RemediationLayerDescriptor): void {
  if (registry.has(d.name)) {
    console.warn(
      `[remediationLayerRegistry] duplicate registration for "${d.name}"; overwriting.`,
    );
  }
  registry.set(d.name, d);
}

/** Look up a layer descriptor by name. Returns undefined if not registered. */
export function getRemediationLayer(
  name: string,
): RemediationLayerDescriptor | undefined {
  return registry.get(name);
}

/** All registered layer descriptors (read-only snapshot). */
export function listRemediationLayers(): readonly RemediationLayerDescriptor[] {
  return Array.from(registry.values());
}

/** Clear the registry. ONLY for tests. */
export function _clearRemediationLayerRegistry(): void {
  registry.clear();
}
