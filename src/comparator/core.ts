import type { Binding } from "../bindings/validator.js";
import type { Z3Value } from "../z3/modelParser.js";

export interface RuntimeValueLite {
  kind: string;
  numberValue?: number | null;
  stringValue?: string | null;
  boolValue?: boolean | null;
}

export interface GapReport {
  kind:
    | "ieee_specials"
    | "int_overflow"
    | "bool_coercion"
    | "null_undefined"
    | "path_not_taken"
    | "outcome_mismatch"
    | "invalid_binding";
  smtConstant: string;
  explanation: string;
  smtValue?: Z3Value;
  runtimeValue?: RuntimeValueLite;
}

export interface ComparatorInput {
  binding: Binding;
  witness: Z3Value;
  runtimeValue: RuntimeValueLite;
}

export type ComparatorAgent = (input: ComparatorInput) => GapReport | null;
