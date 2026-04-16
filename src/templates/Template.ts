import Parser from "tree-sitter";

export interface TemplateResult {
  signalLine: number;
  signalType: string;
  smt2: string;
  claim: string;
  principle: string;
  confidence: "high" | "low";
}

export interface TemplateContext {
  functionName: string;
  filePath: string;
  paramNames: Set<string>;
  pathConditions: string[];
}

export interface Template {
  readonly name: string;
  match(node: Parser.SyntaxNode, ctx: TemplateContext): TemplateResult | null;
}
