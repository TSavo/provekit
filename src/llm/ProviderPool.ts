import { LLMProvider, LLMRequestOptions, LLMResponse, LLMStreamEvent } from "./Provider";

export interface PooledProvider {
  provider: LLMProvider;
  maxConcurrency: number;
  priority: number;
}

export class ProviderPool implements LLMProvider {
  readonly name = "pool";
  private providers: PooledProvider[];
  private activeByProvider: Map<string, number> = new Map();
  private totalRequests = 0;
  private totalFailovers = 0;

  constructor(providers: PooledProvider[]) {
    this.providers = providers.sort((a, b) => a.priority - b.priority);
    for (const p of this.providers) {
      this.activeByProvider.set(p.provider.name, 0);
    }
    console.log(`[pool] Initialized with ${this.providers.length} providers:`);
    for (const p of this.providers) {
      console.log(`[pool]   ${p.provider.name}: max ${p.maxConcurrency} concurrent, priority ${p.priority}`);
    }
  }

  async complete(prompt: string, options: LLMRequestOptions): Promise<LLMResponse> {
    this.totalRequests++;
    const errors: { provider: string; error: string }[] = [];

    for (const pooled of this.providers) {
      const active = this.activeByProvider.get(pooled.provider.name) || 0;
      if (active >= pooled.maxConcurrency) {
        console.log(`[pool] ${pooled.provider.name} at capacity (${active}/${pooled.maxConcurrency}), trying next`);
        continue;
      }

      this.activeByProvider.set(pooled.provider.name, active + 1);
      const slotInfo = `${pooled.provider.name} (${active + 1}/${pooled.maxConcurrency})`;

      try {
        console.log(`[pool] Routing to ${slotInfo}`);
        const response = await pooled.provider.complete(prompt, options);
        this.activeByProvider.set(pooled.provider.name, (this.activeByProvider.get(pooled.provider.name) || 1) - 1);
        return response;
      } catch (err: any) {
        this.activeByProvider.set(pooled.provider.name, (this.activeByProvider.get(pooled.provider.name) || 1) - 1);
        this.totalFailovers++;
        const msg = err.message || String(err);
        errors.push({ provider: pooled.provider.name, error: msg });
        console.log(`[pool] ${pooled.provider.name} failed: ${msg.slice(0, 100)}`);
        console.log(`[pool] Failing over to next provider...`);
      }
    }

    // All at capacity — wait for any slot to open and retry
    console.log(`[pool] All providers at capacity or failed. Waiting for a slot...`);
    const response = await this.waitForSlot(prompt, options, errors);
    return response;
  }

  async *stream(prompt: string, options: LLMRequestOptions): AsyncIterable<LLMStreamEvent> {
    const response = await this.complete(prompt, options);
    yield { type: "done", text: response.text };
  }

  private async waitForSlot(
    prompt: string,
    options: LLMRequestOptions,
    priorErrors: { provider: string; error: string }[]
  ): Promise<LLMResponse> {
    const maxWait = 60000;
    const pollInterval = 500;
    const start = Date.now();

    while (Date.now() - start < maxWait) {
      for (const pooled of this.providers) {
        const failedHere = priorErrors.some((e) => e.provider === pooled.provider.name);
        if (failedHere) continue;

        const active = this.activeByProvider.get(pooled.provider.name) || 0;
        if (active < pooled.maxConcurrency) {
          this.activeByProvider.set(pooled.provider.name, active + 1);
          try {
            console.log(`[pool] Slot opened on ${pooled.provider.name}, retrying`);
            const response = await pooled.provider.complete(prompt, options);
            this.activeByProvider.set(pooled.provider.name, (this.activeByProvider.get(pooled.provider.name) || 1) - 1);
            return response;
          } catch (err: any) {
            this.activeByProvider.set(pooled.provider.name, (this.activeByProvider.get(pooled.provider.name) || 1) - 1);
            priorErrors.push({ provider: pooled.provider.name, error: err.message || String(err) });
            console.log(`[pool] ${pooled.provider.name} failed on retry: ${(err.message || "").slice(0, 100)}`);
          }
        }
      }

      await new Promise((r) => setTimeout(r, pollInterval));
    }

    const errorSummary = priorErrors.map((e) => `${e.provider}: ${e.error.slice(0, 80)}`).join("; ");
    throw new Error(`[pool] All providers exhausted after ${maxWait}ms. Errors: ${errorSummary}`);
  }

  getStats(): { totalRequests: number; totalFailovers: number; active: Record<string, number> } {
    const active: Record<string, number> = {};
    for (const [name, count] of this.activeByProvider) {
      active[name] = count;
    }
    return { totalRequests: this.totalRequests, totalFailovers: this.totalFailovers, active };
  }
}
