export interface Binding {
  smtConstant: string;
  sourceLine: number;
  sourceExpr: string;
  sort: string;
}

export interface ValidationResult {
  valid: Binding[];
  invalid: Array<{ binding: Binding; reason: string }>;
}

export function validateBindings(source: string, bindings: Binding[]): ValidationResult {
  const lines = source.split("\n");
  const valid: Binding[] = [];
  const invalid: Array<{ binding: Binding; reason: string }> = [];

  for (const b of bindings) {
    if (b.sourceLine < 1 || b.sourceLine > lines.length) {
      invalid.push({ binding: b, reason: `line ${b.sourceLine} out of range (1..${lines.length})` });
      continue;
    }
    const lineText = lines[b.sourceLine - 1]!;
    if (!textContainsExpression(lineText, b.sourceExpr)) {
      invalid.push({ binding: b, reason: `source_expr ${JSON.stringify(b.sourceExpr)} not found at line ${b.sourceLine}` });
      continue;
    }
    valid.push(b);
  }
  return { valid, invalid };
}

function textContainsExpression(lineText: string, expr: string): boolean {
  const normalize = (s: string) => s.replace(/\s+/g, "");
  return normalize(lineText).includes(normalize(expr));
}
