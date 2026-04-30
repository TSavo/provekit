export interface DagNode<T> {
  key: string;
  data: T;
  dependsOn: string[];
}

export interface DagResult<T, R> {
  key: string;
  data: T;
  result: R;
}

export class DagExecutor<T, R> {
  private nodes: Map<string, DagNode<T>> = new Map();
  private results: Map<string, R> = new Map();
  private pending: Map<string, { resolve: (result: R) => void; promise: Promise<R> }> = new Map();
  private maxConcurrency: number;
  private activeTasks = 0;
  private queue: string[] = [];
  private allResults: DagResult<T, R>[] = [];

  constructor(maxConcurrency: number = 5) {
    this.maxConcurrency = maxConcurrency;
  }

  add(node: DagNode<T>): void {
    this.nodes.set(node.key, node);
    let resolve!: (result: R) => void;
    const promise = new Promise<R>((res) => { resolve = res; });
    this.pending.set(node.key, { resolve, promise });
  }

  async execute(
    worker: (node: DagNode<T>, resolvedDeps: Map<string, R>) => Promise<R>,
    onResolved?: (key: string, result: R) => void
  ): Promise<DagResult<T, R>[]> {
    console.log(`[dag] Executing ${this.nodes.size} nodes, max concurrency ${this.maxConcurrency}`);

    const ready = this.findReady();
    this.queue.push(...ready);

    return new Promise((resolveAll) => {
      const trySchedule = () => {
        while (this.activeTasks < this.maxConcurrency && this.queue.length > 0) {
          const key = this.queue.shift()!;
          this.activeTasks++;

          const node = this.nodes.get(key)!;
          const resolvedDeps = new Map<string, R>();
          for (const dep of node.dependsOn) {
            const depResult = this.results.get(dep);
            if (depResult !== undefined) resolvedDeps.set(dep, depResult);
          }

          console.log(`[dag] Starting ${key} (${node.dependsOn.length} deps resolved, ${this.activeTasks}/${this.maxConcurrency} active)`);

          worker(node, resolvedDeps).then((result) => {
            this.results.set(key, result);
            this.allResults.push({ key, data: node.data, result });
            this.pending.get(key)!.resolve(result);
            this.activeTasks--;

            console.log(`[dag] Resolved ${key} (${this.results.size}/${this.nodes.size} complete)`);

            if (onResolved) onResolved(key, result);

            const newlyReady = this.findNewlyReady(key);
            if (newlyReady.length > 0) {
              console.log(`[dag] Unblocked: ${newlyReady.join(", ")}`);
              this.queue.push(...newlyReady);
            }

            if (this.results.size === this.nodes.size) {
              resolveAll(this.allResults);
            } else {
              trySchedule();
            }
          });
        }
      };

      trySchedule();
    });
  }

  waitFor(key: string): Promise<R> {
    const entry = this.pending.get(key);
    if (!entry) throw new Error(`[dag] Unknown node: ${key}`);
    return entry.promise;
  }

  private findReady(): string[] {
    const ready: string[] = [];
    for (const [key, node] of this.nodes) {
      if (this.results.has(key)) continue;
      if (node.dependsOn.every((dep) => this.results.has(dep))) {
        ready.push(key);
      }
    }
    return ready;
  }

  private findNewlyReady(justResolved: string): string[] {
    const ready: string[] = [];
    for (const [key, node] of this.nodes) {
      if (this.results.has(key)) continue;
      if (this.queue.includes(key)) continue;
      if (!node.dependsOn.includes(justResolved)) continue;
      if (node.dependsOn.every((dep) => this.results.has(dep))) {
        ready.push(key);
      }
    }
    return ready;
  }
}
