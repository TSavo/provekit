import type { ComparatorAgent, GapReport } from "../core.js";
import type { Z3Value } from "../../z3/modelParser.js";

// Fires on IEEE 754 runtime specials (NaN, ±Infinity) regardless of witness sort.
// The underlying point: TS `number` is always IEEE 754, and any operation on a
// Number (including templates that encode denominators as Int) can produce NaN
// or Infinity at runtime. A principle that encodes its constants as Int but
// works on Number-typed variables at runtime is a legitimate encoding gap —
// the SMT sort understates what IEEE 754 allows.
//
// Float-drift comparison (the epsilon check) only makes sense for Real sort,
// so that branch keeps its guard.
export const ieeeSpecialsAgent: ComparatorAgent = ({ binding, witness, runtimeValue }) => {
  // Runtime produced NaN while SMT modeled any concrete sort.
  if (runtimeValue.kind === "nan") {
    return {
      kind: "ieee_specials",
      smtConstant: binding.smtConstant,
      explanation: `SMT ${witness.sort} modeled ${formatWitness(witness)} but runtime produced NaN (IEEE 754). TypeScript \`number\` values carry IEEE semantics regardless of SMT sort — Int/Real encodings that don't anticipate NaN under-constrain reality.`,
      smtValue: witness,
      runtimeValue,
    };
  }

  // Runtime produced ±Infinity.
  if (runtimeValue.kind === "infinity" || runtimeValue.kind === "neg_infinity") {
    return {
      kind: "ieee_specials",
      smtConstant: binding.smtConstant,
      explanation: `SMT ${witness.sort} modeled ${formatWitness(witness)} but runtime produced ${runtimeValue.kind === "infinity" ? "Infinity" : "-Infinity"} (IEEE 754). SMT sorts don't model JavaScript's IEEE-754 infinities.`,
      smtValue: witness,
      runtimeValue,
    };
  }

  // SMT claimed an IEEE special (div_by_zero/nan/±infinity) on the Real side
  // and runtime produced a finite number. Only Real carries these sentinels
  // through parseZ3Model, so this branch is Real-scoped.
  if (
    witness.sort === "Real" &&
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

  // Float-drift check: only meaningful for Real, where SMT's arithmetic is
  // exact and runtime's is IEEE 754.
  if (witness.sort === "Real" && typeof witness.value === "number" && runtimeValue.kind === "number" && typeof runtimeValue.numberValue === "number") {
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
  if (witness.sort === "Int") return witness.value.toString();
  if (witness.sort === "Bool") return String(witness.value);
  if (witness.sort === "String") return JSON.stringify(witness.value);
  if (witness.sort === "Other") return witness.raw;
  return `<${(witness as Z3Value).sort}>`;
}
