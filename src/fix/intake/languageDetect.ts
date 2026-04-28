/**
 * Language detection for principle-library partitioning (task #134).
 *
 * The principle library is partitioned by language under
 * `.provekit/principles/<partition>/`:
 *   - universal/        — applies regardless of language (always loaded)
 *   - typescript/       — TS/JS-only
 *   - cpp/, rust/, java/, python/, go/ — placeholder partitions for the
 *     CDD-spec'd language-specific axiom sets (currently empty; the
 *     SAST is TypeScript-only).
 *
 * `detectLanguages()` inspects a project root for build-system
 * fingerprints and returns the set of languages to load axioms for. A
 * single project can return multiple languages (a TS frontend + Rust
 * backend yields `["typescript", "rust"]`). The returned set is sorted
 * for stable ordering.
 *
 * Per CDD spec § "Language partitioning: principles per language":
 *   universal axioms apply everywhere; per-language axioms apply only
 *   inside their language's universe; the partition's size is inversely
 *   proportional to the language's compile-time safety story.
 */
import { existsSync, readdirSync, statSync } from "fs";
import { join } from "path";

export type Language =
  | "typescript"
  | "javascript"
  | "cpp"
  | "c"
  | "rust"
  | "java"
  | "python"
  | "go";

export const ALL_LANGUAGES: readonly Language[] = [
  "typescript",
  "javascript",
  "cpp",
  "c",
  "rust",
  "java",
  "python",
  "go",
];

/**
 * Top-level partitions present in `.provekit/principles/`. The reader
 * uses an explicit allowlist (NOT "any subdir") so future additions
 * stay opt-in and existing siblings (`disabled/`, runtime `retired/`)
 * never leak into the load set.
 */
export const PRINCIPLE_PARTITIONS: readonly string[] = [
  "universal",
  ...ALL_LANGUAGES,
];

const SCAN_SKIP_DIRS = new Set([
  "node_modules",
  "dist",
  "build",
  "target",
  "out",
  ".git",
  ".provekit",
  ".next",
  ".cache",
  "coverage",
  "__pycache__",
  "venv",
  ".venv",
]);

/**
 * Detect languages present in `projectRoot`.
 *
 * Strategy: each language has a build-system fingerprint that's cheap
 * to check at the project root. For C/C++ the build-file presence
 * alone is ambiguous (a `Makefile` could build either), so we
 * disambiguate by file extension (`.cpp`/`.cc`/`.cxx`/`.hpp` → cpp;
 * `.c`/`.h`-only → c; mixed → both, with cpp listed first).
 *
 * Returns the languages in stable order for cache-key reproducibility.
 */
export function detectLanguages(projectRoot: string): Language[] {
  const found = new Set<Language>();

  // TS vs JS: package.json gates the family; tsconfig.json or any .ts
  // file in the tree promotes to typescript. (A pure JS package with
  // no .ts files yields just "javascript".)
  if (existsSync(join(projectRoot, "package.json"))) {
    if (existsSync(join(projectRoot, "tsconfig.json"))) {
      found.add("typescript");
    } else {
      // Look shallow for any .ts file — a project with .ts files but no
      // tsconfig still wants TS axioms.
      const hasTs = scanForExtension(projectRoot, [".ts", ".tsx", ".mts", ".cts"], 2);
      if (hasTs) found.add("typescript");
      else found.add("javascript");
    }
  }

  if (existsSync(join(projectRoot, "Cargo.toml"))) {
    found.add("rust");
  }

  if (existsSync(join(projectRoot, "go.mod"))) {
    found.add("go");
  }

  if (
    existsSync(join(projectRoot, "pyproject.toml")) ||
    existsSync(join(projectRoot, "requirements.txt")) ||
    existsSync(join(projectRoot, "setup.py")) ||
    existsSync(join(projectRoot, "Pipfile"))
  ) {
    found.add("python");
  }

  if (
    existsSync(join(projectRoot, "pom.xml")) ||
    existsSync(join(projectRoot, "build.gradle")) ||
    existsSync(join(projectRoot, "build.gradle.kts"))
  ) {
    found.add("java");
  }

  // C / C++: build-system presence is ambiguous. Use file extensions to
  // disambiguate; Makefile/CMakeLists.txt alone with no source files of
  // either flavor doesn't promote either.
  const hasCppFiles = scanForExtension(
    projectRoot,
    [".cpp", ".cc", ".cxx", ".hpp", ".hxx"],
    2,
  );
  const hasCFiles = scanForExtension(projectRoot, [".c"], 2);
  const hasCBuild =
    existsSync(join(projectRoot, "CMakeLists.txt")) ||
    existsSync(join(projectRoot, "Makefile")) ||
    existsSync(join(projectRoot, "configure.ac"));
  if (hasCppFiles) found.add("cpp");
  if (hasCFiles && !hasCppFiles) found.add("c");
  // If only a build file exists with no source files, default to cpp
  // (the larger axiom set is the safer guess for a defender pre-load).
  if (hasCBuild && !hasCppFiles && !hasCFiles) found.add("cpp");

  // Stable order: ALL_LANGUAGES order.
  return ALL_LANGUAGES.filter((l) => found.has(l));
}

/**
 * Shallow-walk up to `maxDepth` directory levels under `root`,
 * checking whether any file matches one of `extensions`. Used to
 * disambiguate C/C++ and TS/JS when build files alone aren't enough.
 *
 * Skips heavyweight dirs (node_modules, target, build, ...) so the
 * scan is cheap even on large monorepos.
 */
function scanForExtension(
  root: string,
  extensions: readonly string[],
  maxDepth: number,
): boolean {
  const stack: Array<{ dir: string; depth: number }> = [{ dir: root, depth: 0 }];
  const lowExts = extensions.map((e) => e.toLowerCase());
  while (stack.length > 0) {
    const next = stack.pop();
    if (!next) break;
    const { dir, depth } = next;
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      continue;
    }
    for (const name of entries) {
      if (SCAN_SKIP_DIRS.has(name)) continue;
      const full = join(dir, name);
      let isDir = false;
      try {
        isDir = statSync(full).isDirectory();
      } catch {
        continue;
      }
      if (isDir) {
        if (depth + 1 <= maxDepth) {
          stack.push({ dir: full, depth: depth + 1 });
        }
      } else {
        const lower = name.toLowerCase();
        for (const ext of lowExts) {
          if (lower.endsWith(ext)) return true;
        }
      }
    }
  }
  return false;
}

/**
 * Resolve the partition directories to load for `projectRoot`.
 *
 * Always includes `universal/`. Adds language-specific partitions for
 * each detected language (only if the partition directory exists on
 * disk — no fail if a placeholder hasn't been created yet).
 *
 * The returned list is in load order: universal first, then language
 * partitions in `ALL_LANGUAGES` order.
 *
 * @param principlesDir absolute path to `.provekit/principles/`
 * @param projectRoot   absolute path to the project being analyzed (its
 *                      languages drive the per-language partition selection)
 */
export function resolvePartitionDirs(
  principlesDir: string,
  projectRoot: string,
): string[] {
  const dirs: string[] = [];
  const universal = join(principlesDir, "universal");
  if (existsSync(universal)) dirs.push(universal);
  for (const lang of detectLanguages(projectRoot)) {
    const dir = join(principlesDir, lang);
    if (existsSync(dir)) dirs.push(dir);
  }
  return dirs;
}
