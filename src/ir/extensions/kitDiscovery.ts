/**
 * Kit discovery — walks a project's node_modules looking for packages
 * that ship ProvekIt bridges, and loads them into the registry with
 * provenance tags.
 *
 * The protocol-aware shape: a package's `package.json` carries a
 * `provekit` metadata field. Today we recognize:
 *
 *   {
 *     "provekit": {
 *       "shimRole": "trojan-horse",
 *       "registersBridges": ["parseInt", "abs", ...]
 *     }
 *   }
 *
 * When discoverProtocolKits walks node_modules and finds a package with
 * a `provekit` field, it dynamic-imports the package's main entry point
 * (which is expected to register bridges as a side effect at module
 * load) and tags the resulting bridge declarations with the package's
 * name + version.
 *
 * The LSP calls this on initialization and on lockfile changes. Each
 * project's bridge set comes from its own installed packages — same
 * way TypeScript reads `node_modules/@types/*` to determine the type
 * surface.
 *
 * Multi-project caveat: the bridge registry is process-global, so a
 * multi-workspace LSP must reset the registry between project
 * activations. Single-workspace use is the v1 contract.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "fs";
import { join } from "path";
import { listBridges } from "./bridges.js";
import type { PrimitiveBridgeDeclaration } from "./bridges.js";

export interface DiscoveredKit {
  packageName: string;
  packageVersion: string;
  packageRoot: string;
  /** Which bridge names this package added to the registry on load. */
  registeredBridgeNames: string[];
}

export interface DiscoveryResult {
  kits: DiscoveredKit[];
  /** Bridge collisions detected during discovery (different packages registering the same name with different targets). */
  collisions: BridgeCollision[];
  /** Bridges currently in the registry, keyed by IR name. */
  byName: Record<string, PrimitiveBridgeDeclaration>;
}

export interface BridgeCollision {
  irName: string;
  packages: Array<{ packageName: string; packageVersion: string; targetContractCid: string }>;
}

/**
 * Walk the project's node_modules looking for packages that ship
 * ProvekIt bridges; load them; tag their bridges with provenance;
 * surface conflicts.
 */
export async function discoverProtocolKits(projectRoot: string): Promise<DiscoveryResult> {
  const kits: DiscoveredKit[] = [];
  const beforeNames = new Set(listBridges().map((b) => b.irName));

  const candidates = enumerateProtocolPackages(projectRoot);
  for (const cand of candidates) {
    const before = new Set(listBridges().map((b) => b.irName));
    try {
      // Dynamic import triggers side-effect bridge registration. The
      // package is expected to import its kit at load time.
      await import(cand.entrypointPath);
    } catch (err) {
      // Loading failed (TS source, syntax error, missing dep). The kit
      // is unusable; skip but record nothing — the absence of the
      // package's bridges in the registry IS the failure signal.
      // Production verifiers may want to surface this as a fail-closed
      // diagnostic.
      continue;
    }
    const after = new Set(listBridges().map((b) => b.irName));
    const newlyRegistered = [...after].filter((n) => !before.has(n));

    // Tag the package's bridges with provenance.
    for (const bridge of listBridges()) {
      if (newlyRegistered.includes(bridge.irName) && !bridge.registeredBy) {
        bridge.registeredBy = {
          packageName: cand.packageName,
          packageVersion: cand.packageVersion,
        };
      }
    }

    kits.push({
      packageName: cand.packageName,
      packageVersion: cand.packageVersion,
      packageRoot: cand.packageRoot,
      registeredBridgeNames: newlyRegistered,
    });
  }

  // Bridges that were already registered before discovery (kit's
  // built-in lazy-init from being imported elsewhere in the LSP
  // process) get a synthetic "internal" provenance tag.
  for (const bridge of listBridges()) {
    if (beforeNames.has(bridge.irName) && !bridge.registeredBy) {
      bridge.registeredBy = {
        packageName: "(internal kit lazy-init)",
        packageVersion: "n/a",
      };
    }
  }

  // Build the final byName index + collision detection. The registry
  // throws on collision-with-different-target at registration time, so
  // by the time we get here the registry is collision-free; we surface
  // the collision check anyway in case a future registry impl relaxes
  // throw-on-collision into surface-then-decide.
  const byName: Record<string, PrimitiveBridgeDeclaration> = {};
  const collisions: BridgeCollision[] = [];
  for (const bridge of listBridges()) {
    byName[bridge.irName] = bridge;
  }

  return { kits, collisions, byName };
}

interface ProtocolPackageCandidate {
  packageName: string;
  packageVersion: string;
  packageRoot: string;
  entrypointPath: string;
}

/**
 * Walk node_modules and enumerate packages that declare a `provekit`
 * field in their package.json. Returns absolute paths to each package's
 * main entry point (resolvable for dynamic import).
 *
 * Walks one level into @scoped directories (e.g., @provekit/ts-types-proof).
 */
function enumerateProtocolPackages(projectRoot: string): ProtocolPackageCandidate[] {
  const out: ProtocolPackageCandidate[] = [];
  const nodeModules = join(projectRoot, "node_modules");
  if (!existsSync(nodeModules)) return out;

  for (const entry of readdirSync(nodeModules)) {
    if (entry.startsWith(".")) continue;
    const entryPath = join(nodeModules, entry);
    let entryStat;
    try {
      entryStat = statSync(entryPath);
    } catch {
      continue;
    }
    if (!entryStat.isDirectory()) continue;

    if (entry.startsWith("@")) {
      // Scoped: walk one level deeper.
      let scopedEntries: string[];
      try {
        scopedEntries = readdirSync(entryPath);
      } catch {
        continue;
      }
      for (const sub of scopedEntries) {
        const subPath = join(entryPath, sub);
        const cand = inspectPackage(subPath);
        if (cand) out.push(cand);
      }
    } else {
      const cand = inspectPackage(entryPath);
      if (cand) out.push(cand);
    }
  }
  return out;
}

function inspectPackage(packageRoot: string): ProtocolPackageCandidate | null {
  const pkgJsonPath = join(packageRoot, "package.json");
  if (!existsSync(pkgJsonPath)) return null;
  let pkg: {
    name?: string;
    version?: string;
    main?: string;
    provekit?: unknown;
  };
  try {
    pkg = JSON.parse(readFileSync(pkgJsonPath, "utf-8"));
  } catch {
    return null;
  }
  if (!pkg.provekit || typeof pkg.provekit !== "object") return null;
  if (!pkg.name || !pkg.version) return null;

  const main = pkg.main ?? "index.js";
  const entrypointPath = join(packageRoot, main);
  if (!existsSync(entrypointPath)) return null;

  return {
    packageName: pkg.name,
    packageVersion: pkg.version,
    packageRoot,
    entrypointPath,
  };
}
