// Pure-arithmetic examples for PropertyTestChecker.
// Every parameter is Int/Real/Bool, top-level exported, so Z3 models map
// directly to runtime calls.

export function safeDivide(a: number, b: number): number {
  console.log("safeDivide", { a, b });
  if (b === 0) return 0;
  return a / b;
}

export function divide(a: number, b: number): number {
  console.log("divide", { a, b });
  return a / b;
}

export function absDiff(x: number, y: number): number {
  console.log("absDiff", { x, y });
  const d = x - y;
  return d < 0 ? -d : d;
}

export function clamp(value: number, min: number, max: number): number {
  console.log("clamp", { value, min, max });
  if (value < min) return min;
  if (value > max) return max;
  return value;
}

export function addPositive(a: number, b: number): number {
  console.log("addPositive", { a, b });
  if (a < 0 || b < 0) throw new Error("negative");
  return a + b;
}
