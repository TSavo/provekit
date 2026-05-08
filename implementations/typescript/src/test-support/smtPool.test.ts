import { describe, expect, it } from "vitest";
import { SolverPool } from "./smtPool.js";

describe("SolverPool shutdown", () => {
  it("does not spawn replacement workers while shutting down", async () => {
    const pool = new SolverPool({ binary: "z3", poolSize: 1 });
    const poolInternals = pool as unknown as {
      spawnWorker: () => { process: { kill: (signal?: NodeJS.Signals) => void; killed: boolean } };
    };
    const originalSpawnWorker = poolInternals.spawnWorker.bind(pool);
    const spawnedWorkers: ReturnType<typeof poolInternals.spawnWorker>[] = [];

    poolInternals.spawnWorker = () => {
      const worker = originalSpawnWorker();
      spawnedWorkers.push(worker);
      return worker;
    };

    try {
      await pool.init();
      expect(spawnedWorkers).toHaveLength(1);

      await pool.shutdown();

      expect(spawnedWorkers).toHaveLength(1);
    } finally {
      for (const worker of spawnedWorkers) {
        if (!worker.process.killed) {
          worker.process.kill("SIGKILL");
        }
      }
    }
  });
});
