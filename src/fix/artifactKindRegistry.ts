import type { FixBundle } from "./types.js";

export interface ArtifactKindDescriptor {
  name: string;
  description: string;
  oraclesThatApply: number[];
  isPresent: (artifacts: FixBundle["artifacts"]) => boolean;
  bundleTypeScope: "fix" | "substrate" | "both";
}

const registry = new Map<string, ArtifactKindDescriptor>();

/**
 * Register an artifact kind. Idempotent: duplicate names overwrite (with a warning).
 */
export function registerArtifactKind(d: ArtifactKindDescriptor): void {
  if (registry.has(d.name)) {
    console.warn(`[artifactKindRegistry] duplicate registration for "${d.name}"; overwriting.`);
  }
  registry.set(d.name, d);
}

/** Look up an artifact kind descriptor by name. Returns undefined if not registered. */
export function getArtifactKind(name: string): ArtifactKindDescriptor | undefined {
  return registry.get(name);
}

/** All registered artifact kind descriptors (read-only snapshot). */
export function listArtifactKinds(): readonly ArtifactKindDescriptor[] {
  return Array.from(registry.values());
}

/** Clear the registry. ONLY for tests. */
export function _clearArtifactKindRegistry(): void {
  registry.clear();
}
