/**
 * Native TypeScript source for the runtime-eval lifting demo.
 *
 * ProvekIt lifts this file directly. There is no sibling contract file: the
 * function bodies are the native surface from which function-contract
 * mementos are derived.
 */

export function parseIntOrZero(input: string): number {
  const parsed = Number.parseInt(input, 10);
  if (Number.isNaN(parsed)) {
    return 0;
  }
  return parsed;
}

export function absoluteValue(x: number): number {
  return Math.abs(x);
}
