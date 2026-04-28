import { betterPrompts, type BetterPrompts } from "@wopr-network/better-prompts";
import { SqliteStore } from "@wopr-network/better-prompts/store/sqlite";
import { mkdirSync } from "fs";
import { join } from "path";

const cache = new Map<string, BetterPrompts>();

export function getPromptStore(projectRoot: string): BetterPrompts {
  const existing = cache.get(projectRoot);
  if (existing) return existing;
  const dir = join(projectRoot, ".provekit");
  mkdirSync(dir, { recursive: true });
  const bp = betterPrompts({
    store: new SqliteStore({ path: join(dir, "prompts.db") }),
  });
  cache.set(projectRoot, bp);
  return bp;
}
