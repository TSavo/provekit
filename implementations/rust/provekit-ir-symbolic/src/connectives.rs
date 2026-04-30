//! Logical connective builders. Mirrors `src/ir/connectives.ts`.

use crate::types::IrFormula;

/// Conjunction. Empty -> vacuously true; single -> identity; many -> And.
pub fn and(conjuncts: Vec<IrFormula>) -> IrFormula {
    match conjuncts.len() {
        0 => IrFormula::Atomic {
            predicate: "true".to_string(),
            args: vec![],
        },
        1 => conjuncts.into_iter().next().unwrap(),
        _ => IrFormula::And { conjuncts },
    }
}

/// Disjunction. Empty -> vacuously false; single -> identity; many -> Or.
pub fn or(disjuncts: Vec<IrFormula>) -> IrFormula {
    match disjuncts.len() {
        0 => IrFormula::Atomic {
            predicate: "false".to_string(),
            args: vec![],
        },
        1 => disjuncts.into_iter().next().unwrap(),
        _ => IrFormula::Or { disjuncts },
    }
}

pub fn not(formula: IrFormula) -> IrFormula {
    IrFormula::Not { body: Box::new(formula) }
}

pub fn implies(antecedent: IrFormula, consequent: IrFormula) -> IrFormula {
    IrFormula::Implies {
        antecedent: Box::new(antecedent),
        consequent: Box::new(consequent),
    }
}

/// Biconditional, desugared to `and(implies(a, b), implies(b, a))` to match
/// the TS canonical-FOL grammar (no `iff` variant in IrFormula).
pub fn iff(a: IrFormula, b: IrFormula) -> IrFormula {
    IrFormula::And {
        conjuncts: vec![
            IrFormula::Implies {
                antecedent: Box::new(a.clone()),
                consequent: Box::new(b.clone()),
            },
            IrFormula::Implies {
                antecedent: Box::new(b),
                consequent: Box::new(a),
            },
        ],
    }
}
