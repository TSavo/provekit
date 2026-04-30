/**
 * Producer registry — capability dispatch.
 *
 * Spec: protocol/specs/2026-04-29-workflows-as-first-class-primitive.md
 *
 * Workflows reference work by capability name ("intake", "verify",
 * "patch") rather than by concrete Stage. The registry resolves a
 * capability to its current producer. Swap a producer (different
 * engine, version bump, swarm-imported impl) by re-registering;
 * workflows don't change.
 *
 * v1 is in-memory and single-producer-per-capability. Multi-producer
 * registry (with priority logic, cross-validation dispatch) is a
 * later extension; the interface accommodates it without rework
 * (resolve() returns one Stage today, could return a list later via
 * a separate method).
 */

import type { Action, Stage } from "./types.js";

export interface ActionRegistry {
  /**
   * Bind an Action to a capability name. Throws if already registered;
   * use replace() to overwrite. Parallel to ProducerRegistry but kept
   * separate because Action<I,R> and Stage<I,O> are different shapes —
   * a unified registry would force a union return that callers have to
   * discriminate.
   */
  register<TInput, TResource>(
    capability: string,
    action: Action<TInput, TResource>,
  ): void;

  replace<TInput, TResource>(
    capability: string,
    action: Action<TInput, TResource>,
  ): void;

  resolve<TInput, TResource>(
    capability: string,
  ): Action<TInput, TResource> | null;

  capabilities(): string[];
}

export interface ProducerRegistry {
  /**
   * Bind a Stage to a capability name. Throws if the capability is
   * already registered — re-registration must be explicit (use
   * replace() to overwrite).
   */
  register<TInput, TOutput>(
    capability: string,
    stage: Stage<TInput, TOutput>,
  ): void;

  /**
   * Replace the producer for an existing capability. Idempotent.
   * Distinct from register() so that accidental double-registration
   * fails loudly.
   */
  replace<TInput, TOutput>(
    capability: string,
    stage: Stage<TInput, TOutput>,
  ): void;

  /**
   * Look up the Stage for a capability. Returns null if no producer
   * is registered. Type parameters are caller-asserted — the registry
   * stores erased types and trusts the caller's annotation.
   */
  resolve<TInput, TOutput>(
    capability: string,
  ): Stage<TInput, TOutput> | null;

  /**
   * All currently-registered capability names. Useful for diagnostics
   * and "what can this CA do?" introspection.
   */
  capabilities(): string[];
}

export class InMemoryRegistry implements ProducerRegistry {
  private readonly stages = new Map<string, Stage<unknown, unknown>>();

  register<TInput, TOutput>(
    capability: string,
    stage: Stage<TInput, TOutput>,
  ): void {
    if (this.stages.has(capability)) {
      throw new Error(
        `producer already registered for capability "${capability}"; use replace() to overwrite`,
      );
    }
    this.stages.set(capability, stage as Stage<unknown, unknown>);
  }

  replace<TInput, TOutput>(
    capability: string,
    stage: Stage<TInput, TOutput>,
  ): void {
    this.stages.set(capability, stage as Stage<unknown, unknown>);
  }

  resolve<TInput, TOutput>(
    capability: string,
  ): Stage<TInput, TOutput> | null {
    const stage = this.stages.get(capability);
    return (stage ?? null) as Stage<TInput, TOutput> | null;
  }

  capabilities(): string[] {
    return [...this.stages.keys()].sort();
  }
}

export class InMemoryActionRegistry implements ActionRegistry {
  private readonly actions = new Map<string, Action<unknown, unknown>>();

  register<TInput, TResource>(
    capability: string,
    action: Action<TInput, TResource>,
  ): void {
    if (this.actions.has(capability)) {
      throw new Error(
        `action already registered for capability "${capability}"; use replace() to overwrite`,
      );
    }
    this.actions.set(capability, action as Action<unknown, unknown>);
  }

  replace<TInput, TResource>(
    capability: string,
    action: Action<TInput, TResource>,
  ): void {
    this.actions.set(capability, action as Action<unknown, unknown>);
  }

  resolve<TInput, TResource>(
    capability: string,
  ): Action<TInput, TResource> | null {
    const action = this.actions.get(capability);
    return (action ?? null) as Action<TInput, TResource> | null;
  }

  capabilities(): string[] {
    return [...this.actions.keys()].sort();
  }
}
