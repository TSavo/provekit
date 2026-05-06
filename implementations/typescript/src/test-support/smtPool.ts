/**
 * SMT-LIB solver worker pool for test isolation and performance.
 *
 * Maintains N long-lived solver processes ready to accept SMT-LIB scripts.
 * Each worker uses (push)/(pop) to isolate per-test state without spawning
 * a new process. Intended for z3 and cvc5 in -in (incremental) mode.
 *
 * Usage:
 *   const pool = new SolverPool({ binary: "z3", poolSize: 4 });
 *   const worker = await pool.acquire();
 *   try {
 *     worker.push();
 *     worker.assert("(declare-const x Int)");
 *     const result = await worker.checkSat();  // "sat"|"unsat"|"unknown"
 *     // test assertions
 *   } finally {
 *     worker.pop();
 *     worker.release();
 *   }
 */

import { spawn, type ChildProcess } from "child_process";

export interface SolverWorker {
  /** Send (push) to the solver and increment the nesting depth. */
  push(): void;

  /** Send (assert <formula>). */
  assert(formula: string): void;

  /**
   * Send (check-sat) and parse the response.
   * Returns "sat", "unsat", or "unknown" (including timeouts).
   */
  checkSat(timeoutMs?: number): Promise<"sat" | "unsat" | "unknown">;

  /** Send (pop) to the solver and decrement nesting depth. */
  pop(): void;

  /**
   * Release the worker back to the pool. MUST call pop() first to clean state.
   */
  release(): void;
}

interface PoolWorkerInternal extends Omit<SolverWorker, "release"> {
  process: ChildProcess;
  depth: number;
  buffer: string;
}

export interface SolverPoolOptions {
  binary: string;
  poolSize?: number;
}

export class SolverPool {
  private binary: string;
  private poolSize: number;
  private available: PoolWorkerInternal[] = [];
  private acquiring: Promise<void>[] = [];
  private initialized = false;

  constructor(options: SolverPoolOptions) {
    this.binary = options.binary;
    this.poolSize = options.poolSize ?? 4;
  }

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;

    for (let i = 0; i < this.poolSize; i++) {
      const worker = this.spawnWorker();
      this.available.push(worker);
    }
  }

  async shutdown(): Promise<void> {
    // Kill all available workers
    await Promise.all(this.available.map((w) => this.killWorker(w)));
    this.available = [];
    this.initialized = false;
  }

  async acquire(): Promise<SolverWorker> {
    await this.init();

    // Wait until a worker is available
    while (this.available.length === 0) {
      await new Promise((resolve) => setTimeout(resolve, 10));
    }

    const worker = this.available.pop()!;
    return this.wrapWorker(worker);
  }

  private wrapWorker(internal: PoolWorkerInternal): SolverWorker {
    const self = this;
    return {
      push: () => internal.push(),
      assert: (formula: string) => internal.assert(formula),
      checkSat: (timeoutMs?: number) => internal.checkSat(timeoutMs),
      pop: () => internal.pop(),
      release: () => {
        // Return to the pool
        self.available.push(internal);
      },
    };
  }

  private spawnWorker(): PoolWorkerInternal {
    const process = spawn(this.binary, ["-in"], {
      stdio: ["pipe", "pipe", "pipe"],
      timeout: 60000, // hard limit per process
    });

    const worker: PoolWorkerInternal = {
      process,
      depth: 0,
      buffer: "",
      push: function () {
        if (this.process.stdin) {
          this.process.stdin.write("(push)\n");
        }
        this.depth++;
      },
      assert: function (formula: string) {
        if (this.process.stdin) {
          this.process.stdin.write(`(assert ${formula})\n`);
        }
      },
      checkSat: async function (timeoutMs = 1000) {
        if (this.process.stdin) {
          this.process.stdin.write("(check-sat)\n");
        }
        return new Promise((resolve) => {
          // Reset buffer for this query
          let queryBuffer = "";
          const handler = (data: Buffer) => {
            queryBuffer += data.toString();
            const trimmed = queryBuffer.trim();
            if (trimmed === "sat" || trimmed === "unsat" || trimmed === "unknown") {
              this.process.stdout?.removeListener("data", handler);
              resolve(trimmed as "sat" | "unsat" | "unknown");
            }
          };

          if (this.process.stdout) {
            this.process.stdout.on("data", handler);
          }

          // Timeout: if no response, return unknown
          const timer = setTimeout(() => {
            if (this.process.stdout) {
              this.process.stdout.removeListener("data", handler);
            }
            resolve("unknown");
          }, timeoutMs);

          // Also clean up if process exits
          const closeHandler = () => {
            clearTimeout(timer);
            if (this.process.stdout) {
              this.process.stdout.removeListener("data", handler);
            }
            resolve("unknown");
          };

          this.process.once("close", closeHandler);
        });
      },
      pop: function () {
        if (this.depth > 0 && this.process.stdin) {
          this.process.stdin.write("(pop)\n");
          this.depth--;
        }
      },
    };

    // Set up stderr to discard error output
    if (process.stderr) {
      process.stderr.on("data", () => {
        /* discard */
      });
    }

    // Crash recovery: if process exits, remove from pool
    process.on("exit", () => {
      const idx = this.available.indexOf(worker);
      if (idx >= 0) {
        this.available.splice(idx, 1);
      }
      // Spawn a replacement
      if (this.initialized && this.available.length < this.poolSize) {
        const replacement = this.spawnWorker();
        this.available.push(replacement);
      }
    });

    return worker;
  }

  private async killWorker(worker: PoolWorkerInternal): Promise<void> {
    return new Promise((resolve) => {
      if (worker.process.stdin) {
        worker.process.stdin.write("(exit)\n");
        worker.process.stdin.end();
      }
      const timeout = setTimeout(() => {
        try {
          worker.process.kill("SIGKILL");
        } catch {
          /* ignore */
        }
        resolve();
      }, 1000);

      worker.process.on("close", () => {
        clearTimeout(timeout);
        resolve();
      });
    });
  }
}

// Global singleton instance for tests
let _globalPool: SolverPool | null = null;

export function initGlobalPool(options: SolverPoolOptions): SolverPool {
  if (_globalPool) {
    return _globalPool;
  }
  _globalPool = new SolverPool(options);
  return _globalPool;
}

export async function getGlobalPool(): Promise<SolverPool> {
  if (!_globalPool) {
    _globalPool = new SolverPool({ binary: "z3", poolSize: 4 });
  }
  await _globalPool.init();
  return _globalPool;
}

export async function shutdownGlobalPool(): Promise<void> {
  if (_globalPool) {
    await _globalPool.shutdown();
    _globalPool = null;
  }
}

/**
 * Helper for test use: acquire a worker, wrap lifecycle, auto-release.
 */
export async function withSolverWorker<T>(
  fn: (worker: SolverWorker) => Promise<T>,
  options?: { timeoutMs?: number },
): Promise<T> {
  const pool = await getGlobalPool();
  const worker = await pool.acquire();
  try {
    worker.push();
    const result = await fn(worker);
    return result;
  } finally {
    worker.pop();
    worker.release();
  }
}
