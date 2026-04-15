import Parser from "tree-sitter";
import { createHash } from "crypto";

export interface ParameterType {
  name: string;
  type: string;
}

export interface Signal {
  file: string;
  line: number;
  column: number;
  type: string;
  text: string;
  functionName: string;
  functionSource: string;
  functionStartLine: number;
  functionEndLine: number;
  parameters: ParameterType[];
  returnType: string;
  pathConditions: string[];
  localTypes: Record<string, string>;
  callees: string[];
  calledBy: string[];
}

export function computeSignalHash(signal: Signal): string {
  const content = [
    signal.file,
    signal.functionName,
    signal.functionSource,
    signal.text,
    signal.type,
    ...signal.pathConditions,
    ...signal.parameters.map((p) => `${p.name}:${p.type}`),
    signal.returnType,
    ...Object.entries(signal.localTypes).map(([k, v]) => `${k}:${v}`),
    ...signal.callees.sort(),
  ].join("\n");
  return createHash("sha256").update(content).digest("hex");
}

export interface SignalGenerator {
  readonly name: string;
  readonly async: boolean;
  findSignals(filePath: string, source: string, tree: Parser.Tree): Signal[] | Promise<Signal[]>;
}
