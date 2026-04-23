import type { ComparatorAgent, GapReport } from "../core.js";
import type { Z3Value } from "../../z3/modelParser.js";

export const ieeeSpecialsAgent: ComparatorAgent = ({ binding, witness, runtimeValue }) => {
  if (witness.sort !== "Real") return null;

  if (runtimeValue.kind === "nan") {
    return {
      kind: "ieee_specials",
      smtConstant: binding.smtConstant,
      explanation: `SMT Real modeled ${formatWitness(witness)} but runtime produced NaN (IEEE 754). Z3's Real sort does not model NaN.`,
      smtValue: witness,
      runtimeValue,
    };
  }

  if (runtimeValue.kind === "infinity" || runtimeValue.kind === "neg_infinity") {
    return {
      kind: "ieee_specials",
      smtConstant: binding.smtConstant,
      explanation: `SMT Real modeled ${formatWitness(witness)} but runtime produced ${runtimeValue.kind === "infinity" ? "Infinity" : "-Infinity"} (IEEE 754). Z3's Real sort does not model infinities the same way JavaScript does.`,
      smtValue: witness,
      runtimeValue,
    };
  }

  if (
    (witness.value === "div_by_zero" || witness.value === "nan" || witness.value === "+infinity" || witness.value === "-infinity") &&
    runtimeValue.kind === "number"
  ) {
    return {
      kind: "ieee_specials",
      smtConstant: binding.smtConstant,
      explanation: `SMT Real modeled ${formatWitness(witness)} but runtime produced the finite value ${runtimeValue.numberValue}. Encoding and runtime diverge on special-value handling.`,
      smtValue: witness,
      runtimeValue,
    };
  }

  if (typeof witness.value === "number" && runtimeValue.kind === "number" && typeof runtimeValue.numberValue === "number") {
    const diff = Math.abs(witness.value - runtimeValue.numberValue);
    const scale = Math.max(1, Math.abs(witness.value), Math.abs(runtimeValue.numberValue));
    if (diff / scale > 1e-9) {
      return {
        kind: "ieee_specials",
        smtConstant: binding.smtConstant,
        explanation: `SMT Real value ${witness.value} differs from runtime IEEE value ${runtimeValue.numberValue} beyond float tolerance.`,
        smtValue: witness,
        runtimeValue,
      };
    }
  }

  return null;
};

function formatWitness(witness: Z3Value): string {
  if (witness.sort === "Real") return typeof witness.value === "number" ? witness.value.toString() : witness.value;
  return JSON.stringify(witness);
}
