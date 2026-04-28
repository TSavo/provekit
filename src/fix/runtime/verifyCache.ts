/**
 * Step 6 of the standing-invariant-runtime spec: cache layer.
 *
 * Per docs/specs/2026-04-27-standing-invariant-runtime.md (section 7,
 * "Cache layer") the verifier needs a content-addressable cache so a
 * pre-commit hook only re-runs the expensive path-enumeration / Z3 /
 * decay work for invariants whose bindings touch changed files.
 *
 *   Cache key:   sha256 of (invariant.id + every binding's resolved nodeId
 *                + every binding's resolved content hash).
 *   Cache value: the previous verdict for that invariant.
 *   Invalidation: any binding's resolved nodeId or content hash changes
 *                 → cache miss → re-evaluate.
 *   Storage:     `.provekit/cache/verify.json`. Gitignored, deletable,
 *                rebuilds on next run.
 *
 * The spec phrases the cache value as `PathVerdict[]`, but step 6 lands
 * before steps 4-5's full per-path Z3 verdicts have settled into a
 * stable shape. v1 caches the whole `InvariantVerdict` (the type already
 * exposed by verify.ts) — when path-level verdicts land, this module
 * stores them transparently because they live inside the verdict.
 *
 * No LLM calls live in this path. The cache exists strictly to skip
 * deterministic re-computation when nothing relevant has changed.
 */

import {
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "fs";
import { join } from "path";
import { createHash } from "crypto";
import type { StoredInvariant } from "./invariantStore.js";
import type { BindingResolution, InvariantVerdict, VerifyOptions, VerifyReport } from "./verify.js";
import { verifyAll } from "./verify.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * A cached verdict for one invariant, keyed by its binding fingerprint.
 *
 * `verdict` is aliased to `InvariantVerdict` (the v1 verdict shape from
 * verify.ts) and re-exported here as `VerifyEntry` so downstream callers
 * can refer to "the thing we cache per invariant" without depending on
 * verify.ts's naming. When step 5's PathVerdict[] becomes a first-class
 * field on InvariantVerdict it will be cached transitively.
 */
export type VerifyEntry = InvariantVerdict;

export interface CacheEntry {
  invariantId: string;
  bindingFingerprint: string;
  verdict: VerifyEntry;
  /** ISO-8601 wall time of the last successful evaluation. */
  checkedAt: string;
}

export interface CachedInvariantVerdict extends InvariantVerdict {
  /**
   * Whether this verdict came from the on-disk cache or was freshly
   * computed on this run.
   */
  cacheStatus: "hit" | "miss";
}

export interface CachedVerifyReport {
  verdicts: CachedInvariantVerdict[];
  summary: VerifyReport["summary"] & {
    cacheHits: number;
    cacheMisses: number;
  };
}

interface CacheFile {
  version: 1;
  entries: CacheEntry[];
}

// ---------------------------------------------------------------------------
// Disk layout
// ---------------------------------------------------------------------------

const CACHE_VERSION = 1 as const;

function cacheDir(projectRoot: string): string {
  return join(projectRoot, ".provekit", "cache");
}

function cachePath(projectRoot: string): string {
  return join(cacheDir(projectRoot), "verify.json");
}

/**
 * Make sure `.provekit/cache/` is gitignored. The cache is a build
 * artifact — deleting it must not change correctness, only performance.
 *
 * We only touch .gitignore when (a) it exists and (b) doesn't already
 * mention the cache directory. We never create a .gitignore: a project
 * without one has opted out of source control, and that's its choice.
 */
function ensureCacheGitignored(projectRoot: string): void {
  const gitignorePath = join(projectRoot, ".gitignore");
  if (!existsSync(gitignorePath)) return;

  let content: string;
  try {
    content = readFileSync(gitignorePath, "utf-8");
  } catch {
    return;
  }

  // Match either the bare path or a leading-slash form. Keep it loose:
  // we only care that "some line mentions .provekit/cache/" is true.
  const alreadyIgnored = content
    .split("\n")
    .some((line) => {
      const trimmed = line.trim();
      return (
        trimmed === ".provekit/cache/" ||
        trimmed === ".provekit/cache" ||
        trimmed === "/.provekit/cache/" ||
        trimmed === "/.provekit/cache"
      );
    });
  if (alreadyIgnored) return;

  const suffix = content.endsWith("\n") ? "" : "\n";
  const addition =
    `${suffix}# Verify-cache (step 6 of the standing-invariant runtime).\n` +
    `# Build artifact: deletable, rebuilds on next run.\n` +
    `.provekit/cache/\n`;
  try {
    writeFileSync(gitignorePath, content + addition, "utf-8");
  } catch {
    // Best-effort: if the gitignore is read-only or otherwise unwritable
    // the cache still functions; the user just has a noisy git status.
  }
}

// ---------------------------------------------------------------------------
// Fingerprint
// ---------------------------------------------------------------------------

/**
 * Substrate identity token. Step 5 of the spec made path-level Z3
 * verdicts a load-bearing piece of the cached value: those verdicts
 * depend on the substrate's data_flow graph + on the live source
 * content at each path step. The fingerprint must invalidate when the
 * substrate flips, otherwise a `provekit analyze` rebuild won't
 * surface as a cache miss and the user gets stale "holds" verdicts.
 *
 * Cheap proxy: include the substrate file's mtime + size. Substrate
 * rebuilds are wholesale (the SAST builder writes the db fresh), so
 * mtime changes whenever the graph changes. If the file is missing,
 * we use a sentinel — a later run that builds the substrate will
 * naturally invalidate every entry.
 */
function substrateIdentity(projectRoot: string): string {
  const dbPath = join(projectRoot, ".provekit", "provekit.db");
  if (!existsSync(dbPath)) return "no-substrate";
  try {
    const stat = statSync(dbPath);
    return `${stat.mtimeMs.toFixed(0)}:${stat.size}`;
  } catch {
    return "no-substrate";
  }
}

/**
 * Compute the cache key for an invariant given its currently-resolved
 * bindings. Per the spec: hash of (invariant.id + each binding's
 * resolved nodeId + each binding's resolved content hash).
 *
 * v1+step5 extension: the fingerprint also folds in a substrate
 * identity token (mtime + size of `.provekit/provekit.db`). Path-level
 * Z3 verdicts are part of the cached value now, and they depend on
 * the substrate's data_flow graph plus live source content. Without
 * this, a `provekit analyze` rebuild + an unchanged `verify` invocation
 * would silently return stale path verdicts.
 *
 * Bindings are sorted by smt_constant before hashing so the fingerprint
 * is stable across binding-iteration order. A decayed binding folds its
 * decay marker into the fingerprint — when a previously decayed binding
 * resolves cleanly on a later run (or vice versa), the fingerprint
 * changes and the cache misses, which is the desired behavior.
 */
export function fingerprint(
  invariant: StoredInvariant,
  bindingResolutions: BindingResolution[],
  projectRoot?: string,
): string {
  const byConstant = new Map(
    bindingResolutions.map((r) => [r.smt_constant, r]),
  );

  const sortedBindings = [...invariant.bindings].sort((a, b) =>
    a.smt_constant.localeCompare(b.smt_constant),
  );

  const parts = sortedBindings.map((b) => {
    const r = byConstant.get(b.smt_constant);
    return {
      c: b.smt_constant,
      file: b.node.filePath,
      hash: b.node.nodeHash,
      startLine: b.node.startLine,
      endLine: b.node.endLine,
      status: r?.status ?? "unknown",
      decayKind: r?.decayKind ?? null,
    };
  });

  const payload = JSON.stringify({
    id: invariant.id,
    bindings: parts,
    substrate: projectRoot ? substrateIdentity(projectRoot) : null,
  });
  return createHash("sha256").update(payload).digest("hex").slice(0, 32);
}

// ---------------------------------------------------------------------------
// Load / save
// ---------------------------------------------------------------------------

/**
 * Load the on-disk cache. Returns an empty Map when the cache file
 * doesn't exist, is corrupt, or carries a version we don't understand.
 *
 * Map key is `${invariantId}:${bindingFingerprint}` so two cache entries
 * for the same invariant under different fingerprints (e.g., a recent
 * good fingerprint and a previously cached miss) don't collide. v1
 * doesn't bother with multi-fingerprint retention but the key shape
 * leaves room for it.
 */
export function loadCache(projectRoot: string): Map<string, CacheEntry> {
  const path = cachePath(projectRoot);
  if (!existsSync(path)) return new Map();

  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, "utf-8"));
  } catch {
    return new Map();
  }

  if (
    !parsed ||
    typeof parsed !== "object" ||
    (parsed as CacheFile).version !== CACHE_VERSION ||
    !Array.isArray((parsed as CacheFile).entries)
  ) {
    return new Map();
  }

  const out = new Map<string, CacheEntry>();
  for (const entry of (parsed as CacheFile).entries) {
    if (
      !entry ||
      typeof entry.invariantId !== "string" ||
      typeof entry.bindingFingerprint !== "string"
    ) {
      continue;
    }
    out.set(cacheKey(entry.invariantId, entry.bindingFingerprint), entry);
  }
  return out;
}

/**
 * Persist the cache to disk. Creates `.provekit/cache/` if necessary
 * and ensures the project's .gitignore (if it has one) excludes the
 * cache directory. Writes atomically via write-then-rename so a
 * crashed invocation doesn't leave a half-formed file.
 */
export function saveCache(
  projectRoot: string,
  cache: Map<string, CacheEntry>,
): void {
  mkdirSync(cacheDir(projectRoot), { recursive: true });
  ensureCacheGitignored(projectRoot);

  const file: CacheFile = {
    version: CACHE_VERSION,
    entries: [...cache.values()].sort((a, b) =>
      a.invariantId.localeCompare(b.invariantId),
    ),
  };

  const path = cachePath(projectRoot);
  const tmp = `${path}.tmp`;
  writeFileSync(tmp, JSON.stringify(file, null, 2) + "\n", "utf-8");
  // Use rename for atomicity on POSIX. fs.renameSync overwrites on Linux/mac.
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const { renameSync } = require("fs");
  renameSync(tmp, path);
}

function cacheKey(invariantId: string, fingerprint: string): string {
  return `${invariantId}:${fingerprint}`;
}

// ---------------------------------------------------------------------------
// verifyAllCached
// ---------------------------------------------------------------------------

/**
 * Wrapper around `verifyAll` that consults the on-disk cache. Each
 * invariant's verdict is computed once by `verifyAll`, then either:
 *
 *   - kept fresh and tagged `cacheStatus: "miss"` when the binding
 *     fingerprint differs from the cached one (or no cached entry
 *     exists), AND
 *   - replaced by the cached verdict and tagged `cacheStatus: "hit"`
 *     when the fingerprints match.
 *
 * v1 takes the simple path: `verifyAll` runs every invariant
 * unconditionally, and the cache layer chooses between "use the fresh
 * result" and "use the cached result" per invariant. When step 5's Z3
 * checker lands and per-path checking becomes the dominant cost, this
 * wrapper will short-circuit BEFORE running path enumeration / Z3 on
 * cache hits. The fingerprint contract stays the same; only the
 * placement of the early-return moves.
 *
 * After all verdicts settle, the cache is rewritten with the latest
 * (fingerprint, verdict) pair per invariant. Stale entries for
 * invariants no longer on disk get dropped on this rewrite.
 */
export async function verifyAllCached(
  projectRoot: string,
  options: VerifyOptions = {},
): Promise<CachedVerifyReport> {
  const cache = loadCache(projectRoot);
  const fresh = await verifyAll(projectRoot, options);

  const next = new Map<string, CacheEntry>();
  const verdicts: CachedInvariantVerdict[] = [];

  for (const v of fresh.verdicts) {
    const fp = fingerprint(v.invariant, v.bindings, projectRoot);
    const key = cacheKey(v.invariant.id, fp);
    const hit = cache.get(key);

    let verdict: InvariantVerdict;
    let cacheStatus: "hit" | "miss";
    let checkedAt: string;

    if (hit) {
      verdict = hit.verdict;
      cacheStatus = "hit";
      checkedAt = hit.checkedAt;
    } else {
      verdict = v;
      cacheStatus = "miss";
      checkedAt = new Date().toISOString();
    }

    verdicts.push({ ...verdict, cacheStatus });
    next.set(key, {
      invariantId: v.invariant.id,
      bindingFingerprint: fp,
      verdict,
      checkedAt,
    });
  }

  // Persist the rebuilt cache. Entries for retired or deleted invariants
  // fall away naturally because we only re-insert keys we just observed.
  saveCache(projectRoot, next);

  const cacheHits = verdicts.filter((v) => v.cacheStatus === "hit").length;
  const cacheMisses = verdicts.length - cacheHits;

  // Recompute summary off the verdicts we ACTUALLY return — for cache
  // hits, the displayed verdict is the cached one, so its status (not
  // the freshly-computed status) governs the summary counts. The two
  // will agree as long as the fingerprint contract is honored, but the
  // recompute keeps display + counts internally consistent if a cache
  // entry's status diverges (e.g., schema migration or a cache that
  // outlives a verdict-shape change).
  const summary = {
    total: verdicts.length,
    holds: verdicts.filter((v) => v.status === "holds").length,
    decayed: verdicts.filter((v) => v.status === "decayed").length,
    violated: verdicts.filter((v) => v.status === "violated").length,
    cacheHits,
    cacheMisses,
  };

  return {
    verdicts,
    summary,
  };
}
