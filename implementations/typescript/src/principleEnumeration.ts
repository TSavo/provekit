/**
 * Partition-aware principle file enumeration (task #134).
 *
 * The principle library is partitioned by language:
 *   .provekit/principles/
 *     universal/   (always loaded)
 *     typescript/  (loaded if TS detected)
 *     cpp/, rust/, java/, python/, go/  (loaded per language detection)
 *     disabled/    (NEVER loaded — historical retired entries)
 *
 * All read-side consumers (PrincipleStore, runLint, recognize stages,
 * harvest recognize, validate-library) call `enumeratePrincipleFiles()`
 * to get the flat list of `.dsl` and `.json` paths to load. This avoids
 * each consumer open-coding the partition walk and forgetting one.
 *
 * Write-side policy: principles minted at runtime (`PrincipleStore.add`,
 * `harvest/promote`) write into the partition they were tagged with at
 * derivation time, defaulting to `universal/` if no language tag was
 * recorded. See `resolveWritePartition()`.
 */
import { existsSync, readdirSync, statSync } from "fs";
import { join } from "path";
import {
  Language,
  PRINCIPLE_PARTITIONS,
  resolvePartitionDirs,
} from "./fix/intake/languageDetect.js";

export interface EnumerationResult {
  /** Absolute paths of `.dsl` files to evaluate. */
  dslPaths: string[];
  /** Absolute paths of `.json` files describing each principle. */
  jsonPaths: string[];
  /** The partition directories that contributed (in load order). */
  partitionDirs: string[];
}

export interface EnumerateOptions {
  /**
   * Project root used for language detection. Defaults to the parent
   * of `principlesDir` (i.e., the project that owns the library).
   */
  projectRoot?: string;
  /**
   * If true, walk EVERY known partition (universal + every language)
   * instead of detection-gated. Used by `runLint` when called against
   * a directory whose language we want to ignore (e.g., the curation
   * gate `validate-library.ts` always wants every principle exercised).
   */
  loadAllPartitions?: boolean;
  /**
   * If true, also walk a top-level flat library (no partition subdirs).
   * Backward-compatibility shim for libraries that haven't migrated
   * yet. Defaults to true: a flat `.dsl` at `principlesDir/foo.dsl` is
   * still picked up, but the partition layout is preferred.
   */
  includeFlatRoot?: boolean;
}

/**
 * Enumerate principle files under `principlesDir`, partition-aware.
 *
 * Algorithm:
 *   1. Determine partition dirs to walk:
 *      - if `loadAllPartitions`: every entry in PRINCIPLE_PARTITIONS
 *      - else: universal/ + detected-language partitions
 *   2. (optional) Add the flat root for backward compatibility.
 *   3. Read `.dsl` and `.json` files from each. Files only — no recursion.
 *
 * Never includes `disabled/` or runtime `retired/`. The allowlist is
 * explicit (PRINCIPLE_PARTITIONS); arbitrary subdirs are not loaded.
 */
export function enumeratePrincipleFiles(
  principlesDir: string,
  opts: EnumerateOptions = {},
): EnumerationResult {
  const dslPaths: string[] = [];
  const jsonPaths: string[] = [];
  const partitionDirs: string[] = [];

  if (!existsSync(principlesDir)) {
    return { dslPaths, jsonPaths, partitionDirs };
  }

  let dirsToWalk: string[];
  if (opts.loadAllPartitions) {
    dirsToWalk = PRINCIPLE_PARTITIONS.map((p) => join(principlesDir, p)).filter(
      (d) => existsSync(d),
    );
  } else {
    const projectRoot =
      opts.projectRoot ?? defaultProjectRootFor(principlesDir);
    dirsToWalk = resolvePartitionDirs(principlesDir, projectRoot);
  }

  for (const dir of dirsToWalk) {
    partitionDirs.push(dir);
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      continue;
    }
    for (const name of entries) {
      const full = join(dir, name);
      try {
        if (!statSync(full).isFile()) continue;
      } catch {
        continue;
      }
      if (name.endsWith(".dsl")) dslPaths.push(full);
      else if (name.endsWith(".json")) jsonPaths.push(full);
    }
  }

  // Backward-compat: also pick up top-level flat files (if any). v2
  // migration moved everything into partitions, but a stale checkout
  // or a hand-written user override might still drop a file at the
  // root. We honor that.
  const includeFlat = opts.includeFlatRoot ?? true;
  if (includeFlat) {
    let rootEntries: string[];
    try {
      rootEntries = readdirSync(principlesDir);
    } catch {
      rootEntries = [];
    }
    for (const name of rootEntries) {
      const full = join(principlesDir, name);
      try {
        if (!statSync(full).isFile()) continue;
      } catch {
        continue;
      }
      if (name.endsWith(".dsl")) dslPaths.push(full);
      else if (name.endsWith(".json")) jsonPaths.push(full);
    }
  }

  // Stable sort for deterministic load order across consumers.
  dslPaths.sort();
  jsonPaths.sort();

  return { dslPaths, jsonPaths, partitionDirs };
}

/**
 * Resolve the partition a newly-minted principle should be written to.
 *
 * Write policy:
 *   - if `language` is provided and is a known partition → that language dir
 *   - else → `universal/`
 *
 * `add()` and `harvest/promote` callers should pass the language tag
 * they captured at derivation time. Without one, `universal/` is the
 * conservative default: a candidate principle that turns out to be
 * language-specific can be moved later, but a candidate filed under a
 * specific language that turns out to be universal would be invisible
 * to projects in other languages.
 */
export function resolveWritePartition(
  principlesDir: string,
  language?: Language | "universal",
): string {
  const partition = language && PRINCIPLE_PARTITIONS.includes(language)
    ? language
    : "universal";
  return join(principlesDir, partition);
}

/**
 * Best-effort default for `projectRoot` given a `principlesDir`.
 *
 * `.provekit/principles/` lives at `<projectRoot>/.provekit/principles`,
 * so the project root is two `dirname`s up. If the path doesn't match
 * that shape, fall back to `principlesDir` itself (which yields no
 * detected languages, only `universal/`).
 */
function defaultProjectRootFor(principlesDir: string): string {
  const segments = principlesDir.split(/[\\/]/);
  if (
    segments.length >= 2 &&
    segments[segments.length - 1] === "principles" &&
    segments[segments.length - 2] === ".provekit"
  ) {
    return segments.slice(0, -2).join("/") || "/";
  }
  return principlesDir;
}
