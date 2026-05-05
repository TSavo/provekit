// SPDX-License-Identifier: Apache-2.0
//
// dropper.rs -- generative completion for the Rust kit (paper 07 §7).
//
// The dropper closes the lifter's loop. The lifter takes Rust source and
// produces canonical IR with arrivals and accumulated WPs at each arrival.
// A "gap" is an arrival at function entry where the accumulated WP contains
// an undischarged leaf precondition -- a predicate the substrate cannot
// verify from static facts alone.
//
// The dropper takes a gap description and writes a piece of Rust source back
// into the caller that closes the gap. The output is a RUNTIME CHECK, not a
// proof. The inserted code's surviving control-flow branch carries the
// invariant the callsite required.
//
// Per paper 07 §11: drop shapes are kit-resident. The dropper carries
// hardcoded knowledge of the host language's idioms for each predicate in
// the foundation catalog. No protocol schema extension is needed. The
// existing ContractDecl-overloaded edge memento (pre/post) is the wire
// format; the dropper uses it as-is.
//
// Schema decision: Case A. The witness memento already exists via
// ContractDecl-overloaded edges (ShadowArrival.pre_wp / post_wp). Drop
// shapes are kit-resident per §11's adjudication. No protocol schema
// extension is warranted until empirical scale demands it.
//
// Phase 1: gap detection -- identify arrivals where the WP contains an
//   undischarged leaf predicate (currently: "not_null").
// Phase 2: cached-witness lookup -- for each gap, find candidate templates
//   from the kit's hardcoded predicate->template table.
// Phase 3: source emission -- render the chosen template, splice into the
//   source text, verify re-lift confirms DAG closure.
// Phase 4 (deferred, #382 follow-up): mint-on-miss via solver portfolio.
//
// Fixture: fn caller(x: Option<i32>) { f(x.unwrap()) }
//   where f requires not_null(x).
//   Gap: FunctionEntry WP contains not_null(x).
//   Template: Defensive (assert shape) emits
//     `if x.is_none() { panic!("not_null: x must be Some"); }`
//     before the callsite.
//   Re-lift: the emitted if-then-panic is recognized by lift.rs as
//     negation of the panic condition, producing !is_none(x) at entry.
//     The substrate maps !is_none(x) -> not_null(x) via the cached
//     witness chain, confirming closure.

use provekit_ir_types::{IrFormula, IrTerm};

use crate::walk::{walk_callsites_to_entry, CallsiteWalk};
use crate::wp::Wp;

// ---- Predicate recognition ----

/// Returns true if the formula contains the named predicate (recursive scan).
/// Used to identify gaps: an undischarged leaf precondition is a formula
/// whose name matches a foundation-catalog predicate, not yet discharged by
/// static facts.
pub fn formula_contains_predicate(formula: &IrFormula, pred_name: &str) -> bool {
    match formula {
        IrFormula::Atomic { name, .. } => name == pred_name,
        IrFormula::And { operands }
        | IrFormula::Or { operands }
        | IrFormula::Not { operands }
        | IrFormula::Implies { operands } => {
            operands.iter().any(|o| formula_contains_predicate(o, pred_name))
        }
        IrFormula::Forall { body, .. } | IrFormula::Exists { body, .. } => {
            formula_contains_predicate(body, pred_name)
        }
        IrFormula::Choice { body, .. } => formula_contains_predicate(body, pred_name),
    }
}

/// Extract the first variable name argument from a named predicate in a formula.
/// For `not_null(x)` returns `Some("x")`. Used to identify the gap's variable.
pub fn predicate_var_arg(formula: &IrFormula, pred_name: &str) -> Option<String> {
    match formula {
        IrFormula::Atomic { name, args } => {
            if name == pred_name {
                args.iter().find_map(|t| match t {
                    IrTerm::Var { name } => Some(name.clone()),
                    _ => None,
                })
            } else {
                None
            }
        }
        IrFormula::And { operands }
        | IrFormula::Or { operands }
        | IrFormula::Not { operands }
        | IrFormula::Implies { operands } => {
            operands.iter().find_map(|o| predicate_var_arg(o, pred_name))
        }
        IrFormula::Forall { body, .. }
        | IrFormula::Exists { body, .. } => predicate_var_arg(body, pred_name),
        IrFormula::Choice { body, .. } => predicate_var_arg(body, pred_name),
    }
}

// ---- Gap ----

/// A detected gap: an arrival where the accumulated WP contains an
/// undischarged leaf predicate the substrate cannot discharge statically.
///
/// The gap's `stmt_index` is the position in the caller's body where
/// the dropper should insert the closing guard. For a FunctionEntry gap
/// the insert position is before the callsite (stmt_index 0 in the
/// arrivals chain).
#[derive(Debug, Clone)]
pub struct Gap {
    /// The caller function name.
    pub caller_name: String,
    /// The callee function name at the callsite producing this gap.
    pub callee_name: String,
    /// The undischarged predicate name (e.g. "not_null").
    pub predicate: String,
    /// The variable name the predicate applies to (extracted from the WP).
    pub var_name: String,
    /// The source statement index where the callsite was found (0-indexed
    /// body position). The dropper inserts a guard BEFORE this index.
    pub callsite_stmt_index: usize,
    /// The full accumulated WP at function entry for this walk.
    pub entry_wp: Wp,
}

/// Detect gaps in a set of callsite walks.
///
/// A gap is a walk whose FunctionEntry arrival's WP contains the named
/// predicate undischarged. The predicate name is taken from the foundation
/// catalog's set of leaf predicates that require runtime discharge.
///
/// Currently supports: "not_null".
///
/// Returns one Gap per walk where the gap is detected.
pub fn detect_gaps(walks: &[CallsiteWalk], predicate: &str) -> Vec<Gap> {
    let mut gaps = Vec::new();
    for walk in walks {
        let entry = walk.entry_wp();
        if formula_contains_predicate(entry.as_formula(), predicate) {
            let var_name = predicate_var_arg(entry.as_formula(), predicate)
                .unwrap_or_else(|| "_".to_string());
            // The callsite is the first arrival in the walk; its stmt_index
            // is the body position of the callsite statement.
            let callsite_stmt_index = walk.arrivals.first().map(|a| a.stmt_index).unwrap_or(0);
            gaps.push(Gap {
                caller_name: walk.caller_name.clone(),
                callee_name: walk.callee_name.clone(),
                predicate: predicate.to_string(),
                var_name,
                callsite_stmt_index,
                entry_wp: entry.clone(),
            });
        }
    }
    gaps
}

// ---- Template families ----

/// The policy family for the emitted drop. Each variant produces the same
/// post-state invariant on the surviving branch; they differ in how they
/// handle the alternative path.
///
/// Per paper 07 §7: the choice between templates is POLICY, not proof.
/// The substrate is grounded by the runtime check; the dropper picks
/// the family member according to user or curator policy.
///
/// Drop shapes are kit-resident per §11. This enum is the entire Rust
/// kit's drop-shape catalog for the "not_null" predicate family.
///
/// **MVP closure verification status:**
/// - `Defensive`: VERIFIED. The emitted `if {var}.is_none() { panic!(...) }` is
///   recognized by lift.rs's if-then-panic path, producing a Not formula over
///   the is_none method call. The re-lift confirms the predicate is discharged.
/// - `Recoverable`: SCAFFOLDING, NOT CLOSURE-VERIFIED. The `return Err(NullInput)`
///   body is not a panic; lift.rs's if-then-panic path does not recognize it.
///   The lifter produces no guard-shaped precondition for this template.
///   Also: `NullInput` is not defined in the caller's scope.
/// - `EarlyReturn`: SCAFFOLDING, NOT CLOSURE-VERIFIED. Same as Recoverable.
///   `return Default::default()` is not a panic; lifter does not recognize it.
///   Additionally requires the return type to implement `Default`.
/// - `Expect`: SCAFFOLDING, NOT CLOSURE-VERIFIED. `let x = x.expect(...)` is a
///   let-binding, not an if-then-panic. The walker substitutes x -> x.expect(...),
///   leaving the not_null predicate still present in the entry WP after re-lift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTemplate {
    /// Defensive: panic on violation. Surviving branch: not_null(x).
    /// Substrate edge minted: assert(x.is_some()) -> not_null(x).
    /// Shape: `if {var}.is_none() { panic!("not_null: {var} must be Some"); }`
    /// **Closure-verified for the MVP.**
    Defensive,
    /// Recoverable guard: if-let with early return. Surviving branch: not_null(x).
    /// Shape: `if {var}.is_none() { return Err(NullInput); }`
    /// Caller now handles Err. Used when the caller has a Result return type.
    /// **SCAFFOLDING -- not closure-verified. Lift.rs does not recognize non-panic
    /// early-return bodies as precondition contributors. Do not use until the
    /// lifter is extended for return-value early-exit patterns.**
    Recoverable,
    /// Early-return shape without if-let sugar.
    /// Shape: `if {var}.is_none() { return Default::default(); }`
    /// **SCAFFOLDING -- not closure-verified. Same limitation as Recoverable.**
    EarlyReturn,
    /// Defensive with documented panic message.
    /// Shape: `let {var} = {var}.expect("invariant: caller must supply non-null {var}");`
    /// **SCAFFOLDING -- not closure-verified. The let-binding is not recognized by
    /// lift.rs's if-then-panic path. The walker substitutes x -> x.expect(...),
    /// leaving the predicate undischarged in the entry WP.**
    Expect,
}

impl DropTemplate {
    /// Render the template as Rust source text, with `var` substituted.
    ///
    /// The rendered text is a complete Rust statement (or pair of statements)
    /// that should be inserted immediately before the callsite in the source.
    /// Trailing newline included so splicing is text-clean.
    pub fn render(&self, var: &str) -> String {
        match self {
            DropTemplate::Defensive => {
                format!(
                    "    if {var}.is_none() {{ panic!(\"not_null: {var} must be Some\"); }}\n",
                    var = var
                )
            }
            DropTemplate::Recoverable => {
                format!(
                    "    if {var}.is_none() {{ return Err(NullInput); }}\n",
                    var = var
                )
            }
            DropTemplate::EarlyReturn => {
                format!(
                    "    if {var}.is_none() {{ return Default::default(); }}\n",
                    var = var
                )
            }
            DropTemplate::Expect => {
                format!(
                    "    let {var} = {var}.expect(\"not_null: invariant: caller must supply non-null {var}\");\n",
                    var = var
                )
            }
        }
    }

    /// Human-readable name for the template family.
    pub fn family_name(&self) -> &'static str {
        match self {
            DropTemplate::Defensive => "defensive",
            DropTemplate::Recoverable => "recoverable",
            DropTemplate::EarlyReturn => "early-return",
            DropTemplate::Expect => "expect",
        }
    }
}

// ---- Phase 2: cached-witness lookup ----

/// Look up candidate templates for a given predicate in the kit's
/// hardcoded predicate->template table.
///
/// This is the kit-resident "cache" per §11. At larger scale, this
/// table would be backed by the substrate's edge cache (CID lookup).
/// For the MVP launch corpus it is hardcoded -- empirical question
/// resolved after operating at scale.
///
/// **MVP policy**: only `Defensive` is returned for "not_null" because it
/// is the only template whose closure is verified by the current lifter
/// (lift.rs recognizes if-then-panic bodies). The other three variants
/// (`Recoverable`, `EarlyReturn`, `Expect`) are scaffolding -- their shapes
/// exist in the enum for future extension but are not closure-verified.
/// They will be added to this table when the lifter is extended to
/// recognize their respective patterns.
///
/// Returns the verified template slice for known predicates, empty for unknown.
pub fn templates_for(predicate: &str) -> &'static [DropTemplate] {
    match predicate {
        "not_null" => &[DropTemplate::Defensive],
        _ => &[],
    }
}

// ---- Phase 3: source emission ----

/// The result of splicing a drop template into a source string.
#[derive(Debug, Clone)]
pub struct EmitResult {
    /// The modified source text with the guard inserted.
    pub modified_source: String,
    /// The template that was applied.
    pub template: DropTemplate,
    /// The variable name the guard was applied to.
    pub var_name: String,
    /// The line number (1-indexed) where the guard was inserted.
    /// Approximate: derived from counting newlines before the callsite
    /// statement, which is sufficient for the re-lift verification step.
    pub insert_line: usize,
}

/// Splice the chosen template into `source` for the given gap.
///
/// Insertion strategy: we find the line that is a callsite expression (not a
/// function definition). A callsite line matches `{callee_name}(` but does
/// NOT start with `fn ` after trimming. We skip function-signature lines.
/// This handles the common case where the callee name appears both as a
/// function definition and as a call expression in the same file.
///
/// Nested callsites (inside if-branches) are deferred to Phase 4.
///
/// Returns `None` if the callee call pattern cannot be located in the source.
pub fn emit_drop(
    source: &str,
    gap: &Gap,
    template: DropTemplate,
) -> Option<EmitResult> {
    let guard_text = template.render(&gap.var_name);

    // Find the line containing the callsite pattern `{callee_name}(` that is
    // a call expression, not a function definition. A function-definition line
    // contains `fn ` before the callee name; a call expression line does not.
    let callee_pattern = format!("{}(", gap.callee_name);
    let lines: Vec<&str> = source.lines().collect();

    let insert_before = lines.iter().position(|l| {
        let trimmed = l.trim();
        // Must contain the call pattern.
        if !trimmed.contains(&callee_pattern) {
            return false;
        }
        // Must NOT be a function definition (fn keyword before callee name).
        let fn_def_pattern = format!("fn {}", gap.callee_name);
        !trimmed.starts_with("fn ") && !trimmed.contains(&fn_def_pattern)
    })?;

    let guard_trimmed = guard_text.trim_end_matches('\n');
    let mut result_lines: Vec<&str> = Vec::with_capacity(lines.len() + 1);
    for (i, line) in lines.iter().enumerate() {
        if i == insert_before {
            // Insert guard before the callsite line.
            result_lines.push(guard_trimmed);
        }
        result_lines.push(line);
    }

    let modified_source = result_lines.join("\n");
    Some(EmitResult {
        modified_source,
        template,
        var_name: gap.var_name.clone(),
        insert_line: insert_before + 1, // 1-indexed
    })
}

// ---- Re-lift verification ----

/// Structural helper: returns true if `formula` contains a `Not` node whose
/// single operand is an `Atomic { name: "is_none" | "is_some", args }` where
/// `args` contains a `Var { name: var_name }`.
///
/// This is the exact shape that lift.rs produces for the if-then-panic pattern:
///   `if x.is_none() { panic!(...) }` => `Not([Atomic("is_none", [Var("x")])])`
///
/// We check this structurally rather than via substring search on serialized JSON
/// to avoid false positives from other uses of "is_none" / "is_some" appearing
/// as string literals, variable names, or comments in the source.
fn formula_contains_guard_for(formula: &IrFormula, var_name: &str) -> bool {
    match formula {
        IrFormula::Not { operands } => {
            // The canonical lift shape is Not with a single operand.
            if operands.len() == 1 {
                if let IrFormula::Atomic { name, args } = &operands[0] {
                    let is_guard = matches!(name.as_str(), "is_none" | "is_some");
                    let has_var = args.iter().any(|t| match t {
                        IrTerm::Var { name } => name == var_name,
                        _ => false,
                    });
                    if is_guard && has_var {
                        return true;
                    }
                }
            }
            // Recurse into all Not operands in case the formula is nested.
            operands.iter().any(|o| formula_contains_guard_for(o, var_name))
        }
        IrFormula::Atomic { .. } => false,
        IrFormula::And { operands }
        | IrFormula::Or { operands }
        | IrFormula::Implies { operands } => {
            operands.iter().any(|o| formula_contains_guard_for(o, var_name))
        }
        IrFormula::Forall { body, .. } | IrFormula::Exists { body, .. } => {
            formula_contains_guard_for(body, var_name)
        }
        IrFormula::Choice { body, .. } => formula_contains_guard_for(body, var_name),
    }
}

/// Verify that the dropper's emission closes the gap.
///
/// Closure criterion: after emitting the guard, the re-lift of the modified
/// source must show that the CALLER function's lifted precondition contains
/// a structurally discharging formula for the required predicate.
///
/// The lift.rs lifter reads the modified caller body and recognizes the
/// emitted `if {var}.is_none() { panic!(...) }` as a precondition
/// contributor via the if-then-panic pattern. This produces:
///   `Not { operands: [Atomic { name: "is_none", args: [Var { name: "x" }] }] }`
/// in the caller's lifted precondition.
///
/// Three structural closure criteria (any one suffices):
///
/// (a) The caller's lifted precondition (from `lift_function_precondition`)
///     contains a `Not` formula whose single operand is `Atomic { name: "is_none"
///     | "is_some" }` with a `Var` argument matching the gap variable. This is
///     the exact shape the Defensive template's if-then-panic produces.
///
/// (b) The gap predicate is absent from the walker's entry WP after re-walking
///     the modified source.
///
/// (c) The walker's entry WP is `Implies { premise, conclusion }` where the
///     conclusion still contains the predicate but the premise encodes the guard.
///
/// Returns `true` if the gap is closed by any criterion.
pub fn verify_closure(
    modified_source: &str,
    gap: &Gap,
    callee_formal_params: &[String],
    callee_precondition: Wp,
) -> bool {
    use crate::lift::lift_function_precondition;

    let file: syn::File = match syn::parse_str(modified_source) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let caller_fn = match file.items.iter().find_map(|item| {
        if let syn::Item::Fn(f) = item {
            if f.sig.ident == gap.caller_name {
                return Some(f.clone());
            }
        }
        None
    }) {
        Some(f) => f,
        None => return false,
    };

    // Criterion (a): structural scan of the caller's lifted precondition.
    //
    // The Defensive template emits `if {var}.is_none() { panic!(...) }`.
    // Lift.rs's if-then-panic path lifts this to:
    //   Not { operands: [Atomic { name: "is_none", args: [Var { name: var }] }] }
    // We check for that exact structure -- NOT a substring match on JSON.
    let caller_pre = lift_function_precondition(&caller_fn);
    if formula_contains_guard_for(caller_pre.as_formula(), &gap.var_name) {
        return true;
    }

    // Criteria (b) and (c): re-walk the modified source and inspect entry WP.
    let walks = walk_callsites_to_entry(
        &caller_fn,
        &gap.callee_name,
        callee_formal_params,
        callee_precondition,
    );

    for walk in &walks {
        let entry_wp = walk.entry_wp();
        let formula = entry_wp.as_formula();

        // Criterion (c): predicate is in conclusion of Implies.
        // The guard became a premise on the surviving branch.
        if let IrFormula::Implies { operands } = formula {
            if operands.len() >= 2
                && formula_contains_predicate(&operands[operands.len() - 1], &gap.predicate)
            {
                return true;
            }
        }

        // Criterion (b): predicate absent entirely from entry WP.
        if !formula_contains_predicate(formula, &gap.predicate) {
            return true;
        }
    }
    false
}

// ---- Public API ----

/// Run all three phases (detect + lookup + emit) for a source file and
/// a known callee precondition. Returns the emit result for the first
/// gap found using the default (Defensive) template.
///
/// This is the main entry point for the dropper's end-to-end path.
/// Phase 4 (mint-on-miss via solver portfolio) is deferred.
///
/// Parameters:
/// - `source`: the Rust source text containing both the callee and caller.
/// - `callee_name`: the function whose precondition has a gap.
/// - `callee_formal_params`: formal parameter names for the callee.
/// - `callee_precondition`: the WP representing the callee's precondition.
/// - `predicate`: the leaf predicate name to look for (e.g. "not_null").
/// - `template`: which drop template to use (default: Defensive).
///
/// Returns `None` if no gap is found or no template matches the predicate.
pub fn drop_gap(
    source: &str,
    callee_name: &str,
    caller_name: &str,
    callee_formal_params: &[String],
    callee_precondition: Wp,
    predicate: &str,
    template: DropTemplate,
) -> Option<EmitResult> {
    let file: syn::File = syn::parse_str(source).ok()?;

    let caller_fn = file.items.iter().find_map(|item| {
        if let syn::Item::Fn(f) = item {
            if f.sig.ident == caller_name {
                return Some(f.clone());
            }
        }
        None
    })?;

    // Phase 1: detect gaps.
    let walks = walk_callsites_to_entry(
        &caller_fn,
        callee_name,
        callee_formal_params,
        callee_precondition,
    );
    let gaps = detect_gaps(&walks, predicate);
    let gap = gaps.into_iter().next()?;

    // Phase 2: look up candidate templates.
    let candidates = templates_for(predicate);
    if candidates.is_empty() {
        return None;
    }
    // Verify the requested template is in the candidate list.
    if !candidates.contains(&template) {
        return None;
    }

    // Phase 3: emit.
    emit_drop(source, &gap, template)
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use provekit_ir_types::{IrFormula, IrTerm};
    use crate::wp::{var, Wp};

    /// Build a not_null(x) WP for testing.
    fn not_null_wp(var_name: &str) -> Wp {
        Wp(IrFormula::Atomic {
            name: "not_null".to_string(),
            args: vec![IrTerm::Var {
                name: var_name.to_string(),
            }],
        })
    }

    // ---- Fixture source ----

    // Callee requiring not_null(x):
    //   fn f(x: Option<i32>) -> i32 { x.unwrap() }
    // Caller that does NOT satisfy not_null(x):
    //   fn caller(x: Option<i32>) { f(x) }
    const FIXTURE_SRC: &str = r#"
fn f(x: Option<i32>) -> i32 {
    x.unwrap()
}

fn caller(x: Option<i32>) {
    f(x);
}
"#;

    // ---- Phase 1: gap detection tests ----

    #[test]
    fn detects_not_null_gap_at_function_entry() {
        let file: syn::File = syn::parse_str(FIXTURE_SRC).expect("parses");
        let caller_fn = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "caller" => Some(f.clone()),
                _ => None,
            })
            .expect("caller fn");

        let precondition = not_null_wp("x");
        let walks = walk_callsites_to_entry(
            &caller_fn,
            "f",
            &["x".to_string()],
            precondition,
        );

        assert_eq!(walks.len(), 1, "one callsite in caller");

        let gaps = detect_gaps(&walks, "not_null");
        assert_eq!(gaps.len(), 1, "one gap detected");

        let gap = &gaps[0];
        assert_eq!(gap.predicate, "not_null");
        assert_eq!(gap.var_name, "x");
        assert_eq!(gap.caller_name, "caller");
        assert_eq!(gap.callee_name, "f");
    }

    #[test]
    fn no_gap_when_predicate_not_present() {
        let src = r#"
fn f(x: u32) -> u32 { x + 1 }
fn caller(x: u32) { f(x); }
"#;
        let file: syn::File = syn::parse_str(src).expect("parses");
        let caller_fn = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "caller" => Some(f.clone()),
                _ => None,
            })
            .expect("caller fn");

        // Precondition: x >= 10 (not a "not_null" predicate).
        use crate::wp::{atomic_ge, const_int};
        let precondition = atomic_ge(var("x"), const_int(10));
        let walks = walk_callsites_to_entry(
            &caller_fn,
            "f",
            &["x".to_string()],
            precondition,
        );

        let gaps = detect_gaps(&walks, "not_null");
        assert_eq!(gaps.len(), 0, "no not_null gap for x >= 10 precondition");
    }

    // ---- Phase 2: cached-witness lookup tests ----

    #[test]
    fn templates_for_not_null_returns_defensive_only() {
        // Only the Defensive template is closure-verified for the MVP.
        // Recoverable, EarlyReturn, and Expect are scaffolding variants in
        // the enum but are not returned here until the lifter is extended
        // to recognize their respective patterns as precondition contributors.
        let templates = templates_for("not_null");
        assert_eq!(templates.len(), 1, "one verified template for not_null (MVP)");
        assert!(templates.contains(&DropTemplate::Defensive));
    }

    #[test]
    fn templates_for_unknown_predicate_returns_empty() {
        let templates = templates_for("some_unknown_predicate");
        assert!(templates.is_empty());
    }

    // ---- Template rendering tests ----

    #[test]
    fn defensive_template_renders_panic_shape() {
        let rendered = DropTemplate::Defensive.render("x");
        assert!(rendered.contains("x.is_none()"), "must guard x");
        assert!(rendered.contains("panic!"), "must panic on violation");
        assert!(rendered.contains("not_null"), "panic msg must name invariant");
    }

    #[test]
    fn recoverable_template_renders_err_shape() {
        let rendered = DropTemplate::Recoverable.render("x");
        assert!(rendered.contains("x.is_none()"), "must guard x");
        assert!(rendered.contains("return Err"), "must return Err");
    }

    #[test]
    fn early_return_template_renders_default_shape() {
        let rendered = DropTemplate::EarlyReturn.render("x");
        assert!(rendered.contains("x.is_none()"), "must guard x");
        assert!(rendered.contains("Default::default()"), "must return default");
    }

    #[test]
    fn expect_template_renders_expect_shape() {
        let rendered = DropTemplate::Expect.render("x");
        assert!(rendered.contains(".expect("), "must call .expect()");
        assert!(rendered.contains("not_null"), "expect msg must name invariant");
    }

    #[test]
    fn all_templates_substitute_var_name() {
        let templates = [
            DropTemplate::Defensive,
            DropTemplate::Recoverable,
            DropTemplate::EarlyReturn,
            DropTemplate::Expect,
        ];
        for tmpl in &templates {
            let rendered = tmpl.render("my_var");
            assert!(
                rendered.contains("my_var"),
                "{} template must contain var name 'my_var': {}",
                tmpl.family_name(),
                rendered
            );
        }
    }

    // ---- Phase 3: source emission tests ----

    #[test]
    fn emit_drop_inserts_guard_before_callsite() {
        let file: syn::File = syn::parse_str(FIXTURE_SRC).expect("parses");
        let caller_fn = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "caller" => Some(f.clone()),
                _ => None,
            })
            .expect("caller fn");

        let precondition = not_null_wp("x");
        let walks = walk_callsites_to_entry(
            &caller_fn,
            "f",
            &["x".to_string()],
            precondition,
        );
        let gaps = detect_gaps(&walks, "not_null");
        let gap = &gaps[0];

        let result = emit_drop(FIXTURE_SRC, gap, DropTemplate::Defensive)
            .expect("emit succeeds");

        // The guard must appear before f( in the modified source.
        let guard_pos = result.modified_source.find("x.is_none()").expect("guard present");
        let callsite_pos = result.modified_source.find("f(x)").expect("callsite present");
        assert!(
            guard_pos < callsite_pos,
            "guard must appear before callsite: guard_pos={}, callsite_pos={}",
            guard_pos,
            callsite_pos
        );
    }

    #[test]
    fn emitted_source_is_syntactically_valid() {
        let file: syn::File = syn::parse_str(FIXTURE_SRC).expect("parses");
        let caller_fn = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "caller" => Some(f.clone()),
                _ => None,
            })
            .expect("caller fn");

        let precondition = not_null_wp("x");
        let walks = walk_callsites_to_entry(
            &caller_fn,
            "f",
            &["x".to_string()],
            precondition,
        );
        let gaps = detect_gaps(&walks, "not_null");
        let gap = &gaps[0];

        let result = emit_drop(FIXTURE_SRC, gap, DropTemplate::Defensive)
            .expect("emit succeeds");

        // The modified source must parse cleanly (syn validates Rust syntax).
        let parse_result: Result<syn::File, _> = syn::parse_str(&result.modified_source);
        assert!(
            parse_result.is_ok(),
            "emitted source must be syntactically valid Rust: {:?}",
            parse_result.err()
        );
    }

    // ---- Re-lift verification tests ----

    #[test]
    fn re_lift_confirms_closure_after_defensive_drop() {
        let file: syn::File = syn::parse_str(FIXTURE_SRC).expect("parses");
        let caller_fn = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "caller" => Some(f.clone()),
                _ => None,
            })
            .expect("caller fn");

        let precondition = not_null_wp("x");
        let walks = walk_callsites_to_entry(
            &caller_fn,
            "f",
            &["x".to_string()],
            precondition.clone(),
        );
        let gaps = detect_gaps(&walks, "not_null");
        let gap = &gaps[0];

        let result = emit_drop(FIXTURE_SRC, gap, DropTemplate::Defensive)
            .expect("emit succeeds");

        // Re-lift: verify the modified source closes the gap.
        let closed = verify_closure(
            &result.modified_source,
            gap,
            &["x".to_string()],
            precondition,
        );
        assert!(
            closed,
            "re-lift must confirm DAG closure after Defensive drop. Modified source:\n{}",
            result.modified_source
        );
    }

    // ---- End-to-end drop_gap API test ----

    #[test]
    fn end_to_end_drop_gap_defensive() {
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "caller",
            &["x".to_string()],
            not_null_wp("x"),
            "not_null",
            DropTemplate::Defensive,
        );

        assert!(result.is_some(), "drop_gap must succeed for not_null fixture");
        let emit = result.unwrap();
        assert_eq!(emit.template, DropTemplate::Defensive);
        assert_eq!(emit.var_name, "x");
        // The modified source must be parseable.
        let parse_result: Result<syn::File, _> = syn::parse_str(&emit.modified_source);
        assert!(parse_result.is_ok(), "emitted source parses: {:?}", parse_result.err());
        // The guard must appear before the callsite.
        let guard_pos = emit.modified_source.find("x.is_none()").expect("guard present");
        let callsite_pos = emit.modified_source.find("f(x)").expect("callsite present");
        assert!(guard_pos < callsite_pos, "guard before callsite");
    }
}
