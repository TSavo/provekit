import type { IrFormula, IrTerm, Sort } from "../../ir/formulas.js";
import type {
  FunctionContractMemento,
  TypeScriptSourceDiagnostic,
  TypeScriptSourceLiftResult,
  TypeScriptSourceRefusal,
} from "./index.js";

export interface TypeScriptSourceVerifyContract extends FunctionContractMemento {
  bridgeSourceSymbol: string;
}

export interface TypeScriptSourceVerifyIrDocument {
  kind: "ir-document";
  ir: TypeScriptSourceVerifyContract[];
  callEdges: [];
  diagnostics: TypeScriptSourceDiagnostic[];
  opacityReport: unknown[];
  refusals: TypeScriptSourceRefusal[];
}

export function normalizeTypeScriptSourceVerifyDocument(
  result: TypeScriptSourceLiftResult,
): TypeScriptSourceVerifyIrDocument {
  return {
    kind: "ir-document",
    ir: result.declarations
      .filter((decl) => !decl.fnName.endsWith(":<source-unit>"))
      .map(normalizeFunctionContractForVerify),
    callEdges: [],
    diagnostics: result.diagnostics,
    opacityReport: result.opacityReport,
    refusals: result.refusals,
  };
}

export function normalizeFunctionContractForVerify(
  contract: FunctionContractMemento,
): TypeScriptSourceVerifyContract {
  return {
    ...contract,
    bridgeSourceSymbol: bridgeSourceSymbol(contract.fnName),
    formalSorts: contract.formalSorts.map(normalizeSortForVerify),
    returnSort: normalizeSortForVerify(contract.returnSort),
    pre: normalizeFormulaForVerify(contract.pre),
    post: normalizeFormulaForVerify(contract.post),
  };
}

function normalizeFormulaForVerify(formula: IrFormula): IrFormula {
  switch (formula.kind) {
    case "atomic":
      return {
        ...formula,
        name: normalizeAtomicName(formula.name),
        args: formula.args.map(normalizeTermForVerify),
      };
    case "and":
    case "or":
    case "not":
    case "implies":
      return { ...formula, operands: formula.operands.map(normalizeFormulaForVerify) };
    case "forall":
    case "exists":
      return {
        ...formula,
        sort: normalizeSortForVerify(formula.sort),
        body: normalizeFormulaForVerify(formula.body),
      };
    case "choice":
      return {
        ...formula,
        sort: normalizeSortForVerify(formula.sort),
        body: normalizeFormulaForVerify(formula.body),
      };
  }
}

function normalizeTermForVerify(term: IrTerm): IrTerm {
  switch (term.kind) {
    case "var":
      return {
        ...term,
        name: term.name === "return_value" ? "result" : term.name,
      };
    case "const":
      return {
        ...term,
        sort: normalizeConstSortForVerify(term.value, term.sort),
      };
    case "ctor":
      return {
        ...term,
        name: normalizeCtorName(term.name),
        args: term.args.map(normalizeTermForVerify),
      };
    case "lambda":
      return {
        ...term,
        paramSort: normalizeSortForVerify(term.paramSort),
        body: normalizeTermForVerify(term.body),
      };
    case "let":
      return {
        ...term,
        bindings: term.bindings.map((binding) => ({
          ...binding,
          boundTerm: normalizeTermForVerify(binding.boundTerm),
        })),
        body: normalizeTermForVerify(term.body),
      };
  }
}

function normalizeSortForVerify(sort: Sort): Sort {
  switch (sort.kind) {
    case "primitive":
      if (sort.name === "Number") return { kind: "primitive", name: "Real" };
      if (sort.name === "Boolean") return { kind: "primitive", name: "Bool" };
      if (sort.name === "Unit") return { kind: "primitive", name: "Int" };
      if (sort.name === "Any" || sort.name === "Unknown" || sort.name === "Null") {
        return { kind: "primitive", name: "Int" };
      }
      return sort;
    case "set":
      return { ...sort, element: normalizeSortForVerify(sort.element) };
    case "tuple":
      return { ...sort, elements: sort.elements.map(normalizeSortForVerify) };
    case "function":
      return {
        ...sort,
        args: sort.args.map(normalizeSortForVerify),
        return: normalizeSortForVerify(sort.return),
      };
    case "dependent":
      return { ...sort, indexSort: normalizeSortForVerify(sort.indexSort) };
    default:
      return sort;
  }
}

function normalizeConstSortForVerify(value: unknown, sort: Sort): Sort {
  if (sort.kind === "primitive" && sort.name === "Number") {
    return Number.isInteger(value) ? { kind: "primitive", name: "Int" } : { kind: "primitive", name: "Real" };
  }
  return normalizeSortForVerify(sort);
}

function normalizeCtorName(name: string): string {
  switch (name) {
    case "ts:add":
      return "+";
    case "ts:sub":
      return "-";
    case "ts:mul":
      return "*";
    case "ts:div":
      return "/";
    case "ts:mod":
      return "mod";
    default:
      return name;
  }
}

function normalizeAtomicName(name: string): string {
  switch (name) {
    case "ts:eq":
      return "=";
    case "ts:ne":
      return "≠";
    case "ts:lt":
      return "<";
    case "ts:le":
      return "≤";
    case "ts:gt":
      return ">";
    case "ts:ge":
      return "≥";
    default:
      return name;
  }
}

function bridgeSourceSymbol(fnName: string): string {
  const withoutParams = fnName.split("(")[0] ?? fnName;
  const afterModule = withoutParams.includes(":")
    ? withoutParams.slice(withoutParams.lastIndexOf(":") + 1)
    : withoutParams;
  return afterModule.split(".").pop() || afterModule;
}
