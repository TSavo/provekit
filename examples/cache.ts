/**
 * A caching layer with time-based expiry.
 * Designed to trigger Phase 4: the TTL/staleness pattern
 * is not covered by P1-P7 seed axioms.
 */

interface CacheEntry<T> {
  value: T;
  cachedAt: number;
  ttlMs: number;
}

const store = new Map<string, CacheEntry<unknown>>();

export function cacheGet<T>(key: string): T | null {
  const entry = store.get(key) as CacheEntry<T> | undefined;
  if (!entry) {
    console.log(`Cache miss: ${key}`);
    return null;
  }

  const age = Date.now() - entry.cachedAt;
  console.log(`Cache hit: ${key}, age=${age}ms, ttl=${entry.ttlMs}ms`);

  // BUG: returns stale value — checks age but doesn't enforce TTL
  return entry.value;
}

export function cacheSet<T>(key: string, value: T, ttlMs: number): void {
  store.set(key, { value, cachedAt: Date.now(), ttlMs });
  console.log(`Cache set: ${key}, ttl=${ttlMs}ms`);
}

export function getUserProfile(userId: string): { name: string; role: string } {
  const cached = cacheGet<{ name: string; role: string }>(`user:${userId}`);
  if (cached) {
    console.log(`Returning cached profile for ${userId}: role=${cached.role}`);
    return cached;
  }

  // Simulate DB fetch
  const profile = { name: "User " + userId, role: "member" };
  cacheSet(`user:${userId}`, profile, 300000); // 5 min TTL
  console.log(`Fetched and cached profile for ${userId}`);
  return profile;
}
