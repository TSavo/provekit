export type Z3Value =
  | { sort: "Real"; value: number | "div_by_zero" | "nan" | "+infinity" | "-infinity" }
  | { sort: "Int"; value: bigint }
  | { sort: "Bool"; value: boolean }
  | { sort: "String"; value: string }
  | { sort: "Other"; raw: string };

export function parseZ3Model(input: string): Map<string, Z3Value> {
  const result = new Map<string, Z3Value>();
  // Z3 emits:
  //   (
  //     (define-fun NAME () SORT VALUE)
  //     ...
  //   )
  // where VALUE may be a literal, a negated literal (- N), or an expression
  // like (/ 1.0 0.0). We do S-expression parsing.
  const tokens = tokenize(input);
  if (tokens.length === 0) return result;
  const tree = parseSexp(tokens, { idx: 0 });
  if (!Array.isArray(tree)) return result;
  for (const entry of tree) {
    if (!Array.isArray(entry)) continue;
    if (entry[0] !== "define-fun") continue;
    const name = entry[1] as string;
    const sort = entry[3] as string;
    const value = entry[4];
    const z3val = interpretValue(sort, value);
    if (z3val) result.set(name, z3val);
  }
  return result;
}

type SexpNode = string | SexpNode[];

function tokenize(s: string): string[] {
  const tokens: string[] = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === " " || c === "\n" || c === "\t" || c === "\r") {
      i++;
      continue;
    }
    if (c === "(" || c === ")") {
      tokens.push(c);
      i++;
      continue;
    }
    if (c === '"') {
      // string literal — scan to closing quote (Z3 doubles internal quotes)
      let j = i + 1;
      let buf = '"';
      while (j < s.length) {
        if (s[j] === '"' && s[j + 1] === '"') {
          buf += '""';
          j += 2;
          continue;
        }
        buf += s[j];
        if (s[j] === '"') {
          j++;
          break;
        }
        j++;
      }
      tokens.push(buf);
      i = j;
      continue;
    }
    // atom
    let j = i;
    while (j < s.length && !/[()\s]/.test(s[j]!)) j++;
    tokens.push(s.slice(i, j));
    i = j;
  }
  return tokens;
}

function parseSexp(tokens: string[], pos: { idx: number }): SexpNode {
  const t = tokens[pos.idx++];
  if (t === "(") {
    const list: SexpNode[] = [];
    while (tokens[pos.idx] !== ")") {
      if (pos.idx >= tokens.length) throw new Error("unclosed paren");
      list.push(parseSexp(tokens, pos));
    }
    pos.idx++; // consume ')'
    return list;
  }
  if (t === ")" || t === undefined) throw new Error(`unexpected token ${t}`);
  return t;
}

function interpretValue(sort: string, value: SexpNode): Z3Value | null {
  if (sort === "Bool") {
    return { sort: "Bool", value: value === "true" };
  }
  if (sort === "Int") {
    if (typeof value === "string") return { sort: "Int", value: BigInt(value) };
    if (Array.isArray(value) && value[0] === "-" && typeof value[1] === "string") {
      return { sort: "Int", value: -BigInt(value[1]) };
    }
    return { sort: "Other", raw: stringify(value) };
  }
  if (sort === "Real") {
    if (typeof value === "string") {
      if (value === "+oo" || value === "oo" || value === "inf") return { sort: "Real", value: "+infinity" };
      if (value === "-oo") return { sort: "Real", value: "-infinity" };
      const n = Number(value);
      if (!Number.isNaN(n)) return { sort: "Real", value: n };
      return { sort: "Other", raw: value };
    }
    if (Array.isArray(value)) {
      if (value[0] === "-" && typeof value[1] === "string") {
        return { sort: "Real", value: -Number(value[1]) };
      }
      if (value[0] === "/" && value.length === 3 && typeof value[1] === "string" && typeof value[2] === "string") {
        const n = Number(value[1]);
        const d = Number(value[2]);
        if (d === 0) return { sort: "Real", value: "div_by_zero" };
        return { sort: "Real", value: n / d };
      }
      return { sort: "Other", raw: stringify(value) };
    }
  }
  if (sort === "String") {
    if (typeof value === "string" && value.startsWith('"') && value.endsWith('"')) {
      return { sort: "String", value: value.slice(1, -1).replace(/""/g, '"') };
    }
  }
  return { sort: "Other", raw: stringify(value) };
}

function stringify(node: SexpNode): string {
  if (typeof node === "string") return node;
  return "(" + node.map(stringify).join(" ") + ")";
}
