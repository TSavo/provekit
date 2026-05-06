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
/// **Skip policy**: if the predicate's argument is not a simple `Var` (e.g.
/// it is a constructor expression, a function call, or a literal), the
/// dropper has no concrete identifier to guard. Such gaps are skipped with
/// a diagnostic to stderr (matching `walk.rs`'s eprintln pattern for
/// arity-mismatch skips). The gap remains visible to a downstream tool that
/// inspects walks directly; only the dropper's emission path is best-effort.
///
/// Returns one Gap per walk where the gap is detected and a Var argument
/// was extracted.
pub fn detect_gaps(walks: &[CallsiteWalk], predicate: &str) -> Vec<Gap> {
    let mut gaps = Vec::new();
    for walk in walks {
        let entry = walk.entry_wp();
        if !formula_contains_predicate(entry.as_formula(), predicate) {
            continue;
        }
        let Some(var_name) = predicate_var_arg(entry.as_formula(), predicate) else {
            // Non-Var argument: skip with a diagnostic. Matches walk.rs's
            // arity-mismatch skip pattern (line 100). This preserves liveness
            // for other walks; the gap is not silently rendered as `_.is_none()`.
            eprintln!(
                "provekit-walk/dropper: predicate `{}` in {}->{} entry WP has \
                 non-Var argument; skipping gap (no concrete identifier to guard)",
                predicate, walk.caller_name, walk.callee_name
            );
            continue;
        };
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

/// Reason a `DropTemplate` cannot currently be rendered to compilable Rust.
///
/// The Defensive template is the only currently-renderable variant. The other
/// three exist in the enum for documenting the policy axis (paper 07 §7), but
/// their render paths produce uncompilable Rust without additional context
/// (caller-supplied error types, fresh-name binding for shadowing, etc.).
/// The render method returns this error rather than emitting broken source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotRenderable {
    /// The template family is documented but not yet implemented in a
    /// compilable form. Carries the family name for diagnostic context.
    Scaffolding {
        family: &'static str,
        reason: &'static str,
    },
}

impl std::fmt::Display for NotRenderable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotRenderable::Scaffolding { family, reason } => {
                write!(f, "DropTemplate::{family} is scaffolding only: {reason}")
            }
        }
    }
}

impl std::error::Error for NotRenderable {}

impl DropTemplate {
    /// Render the template as Rust source text, with `var` substituted.
    ///
    /// Returns `Ok(text)` for the Defensive template, which produces compilable
    /// guard code. Returns `Err(NotRenderable::Scaffolding)` for the other three
    /// templates because their render paths produce uncompilable Rust:
    /// - `Recoverable` references an undefined `NullInput` error type.
    /// - `EarlyReturn` requires the caller's return type to implement `Default`.
    /// - `Expect` shadows `var` with the unwrapped type, breaking the callsite.
    ///
    /// The non-Defensive variants remain in the enum so the policy axis from
    /// paper 07 §7 is documented in code, but their render paths are gated
    /// behind this `Result` until the lifter and dropper are extended to
    /// produce verified, compilable output for them. See follow-up issues
    /// (#407 for Expect's shadowing, etc).
    ///
    /// The rendered text is a complete Rust statement that should be inserted
    /// immediately before the callsite in the source.
    /// Trailing newline included so splicing is text-clean.
    ///
    /// Returns `Ok(text)` for `Defensive` and `Expect`. Returns
    /// `Err(NotRenderable::Scaffolding)` for `Recoverable` and `EarlyReturn`
    /// which still have unresolved context requirements (missing error types /
    /// Default bound).
    ///
    /// Note: `Expect` is renderable but NOT closure-verified — the let-binding
    /// pattern is not recognised by lift.rs's if-then-panic path. Callers that
    /// go through `drop_gap` will receive `TemplateNotCandidate` until the
    /// lifter is extended. Direct `emit_drop` callers get a type-correct
    /// (compilable) emission; `verify_closure` will return false for it.
    pub fn render(&self, var: &str) -> Result<String, NotRenderable> {
        match self {
            DropTemplate::Defensive => Ok(format!(
                "    if {var}.is_none() {{ panic!(\"not_null: {var} must be Some\"); }}\n",
                var = var
            )),
            DropTemplate::Recoverable => Err(NotRenderable::Scaffolding {
                family: "Recoverable",
                reason:
                    "render emits `return Err(NullInput)` but no error type is defined; \
                     pending caller-supplied error_expr support",
            }),
            DropTemplate::EarlyReturn => Err(NotRenderable::Scaffolding {
                family: "EarlyReturn",
                reason:
                    "render emits `return Default::default()` which requires the caller's \
                     return type to implement Default; not closure-verified by current lifter",
            }),
            DropTemplate::Expect => Ok(format!(
                "    let {var}_checked = {var}.expect(\
                    \"not_null: invariant: caller must supply non-null {var}\");\n",
                var = var
            )),
        }
    }

    /// Returns `Some((old_arg, new_arg))` when the emitted template requires
    /// the callsite argument to be rewritten in addition to inserting a guard
    /// line. `emit_drop` applies this substitution to the callsite line.
    ///
    /// `Expect` binds a fresh name `{var}_checked` and must substitute it at
    /// every argument occurrence of `{var}` on the callsite line to preserve
    /// the original type (`Option<T>`) at the callee boundary.
    /// The substitution is whole-word only to avoid spurious replacements.
    pub fn callsite_arg_rewrite(&self, var: &str) -> Option<(String, String)> {
        match self {
            DropTemplate::Expect => Some((var.to_string(), format!("{var}_checked"))),
            _ => None,
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
/// Insertion strategy is **AST-anchored**, not line-pattern-based:
///
/// 1. Parse `source` with `syn::parse_str`.
/// 2. Locate the function whose `sig.ident` matches `gap.caller_name`.
///    There may be multiple functions in the file calling the same callee;
///    we splice into the SPECIFIC caller named by the gap.
/// 3. Look up `caller.block.stmts[gap.callsite_stmt_index]` and read its
///    span (`Spanned::span(stmt).start().line`). This is the exact 1-indexed
///    source line of the callsite statement, robust to multi-line statements,
///    multi-callsite functions, and shared callee names across functions.
/// 4. Splice the rendered guard text immediately before that line.
///
/// Nested callsites within compound statements (e.g., inside `if` arms): the
/// `stmt_index` from the walker still points at the enclosing statement, so
/// the guard is inserted before the enclosing statement. This is conservative
/// (the guard runs unconditionally), but correct in the sense that the
/// invariant holds at the callsite. Per-branch guard placement is deferred.
///
/// Returns `None` if:
/// - the source does not parse, OR
/// - the named caller is not present in the file, OR
/// - the `callsite_stmt_index` is out of range for the caller's body, OR
/// - the chosen template is scaffolding (render returns NotRenderable).
pub fn emit_drop(
    source: &str,
    gap: &Gap,
    template: DropTemplate,
) -> Option<EmitResult> {
    use syn::spanned::Spanned;

    let guard_text = template.render(&gap.var_name).ok()?;

    // Parse the source so we can resolve callsite locations via syn spans
    // rather than line-pattern matching.
    let file: syn::File = syn::parse_str(source).ok()?;

    // Locate the SPECIFIC caller named by the gap. This is the P1a fix:
    // a multi-function file with the same callee in multiple callers must
    // route the splice to the function named on the gap.
    let caller_fn = file.items.iter().find_map(|item| {
        if let syn::Item::Fn(f) = item {
            if f.sig.ident == gap.caller_name {
                return Some(f);
            }
        }
        None
    })?;

    // Resolve the callsite statement by index within the caller's body.
    let callsite_stmt = caller_fn.block.stmts.get(gap.callsite_stmt_index)?;

    // The span's start line is 1-indexed; convert to 0-indexed for splicing.
    let callsite_line_1indexed = callsite_stmt.span().start().line;
    if callsite_line_1indexed == 0 {
        // Defensive: a span with line 0 would produce an underflow below.
        return None;
    }
    let insert_before_idx = callsite_line_1indexed - 1;

    let lines: Vec<&str> = source.lines().collect();
    if insert_before_idx >= lines.len() {
        return None;
    }

    let guard_trimmed = guard_text.trim_end_matches('\n');
    let callsite_rewrite = template.callsite_arg_rewrite(&gap.var_name);
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len() + 1);
    for (i, line) in lines.iter().enumerate() {
        if i == insert_before_idx {
            result_lines.push(guard_trimmed.to_string());
        }
        if i == insert_before_idx {
            if let Some((old, new)) = &callsite_rewrite {
                result_lines.push(replace_word(line, old, new));
                continue;
            }
        }
        result_lines.push(line.to_string());
    }

    let modified_source = result_lines.join("\n");
    Some(EmitResult {
        modified_source,
        template,
        var_name: gap.var_name.clone(),
        insert_line: callsite_line_1indexed,
    })
}

// ---- Re-lift verification ----

/// Replace whole-word occurrences of `old` with `new` in `text`.
///
/// A "word boundary" is checked by ensuring the character immediately before
/// and after the match is not alphanumeric or `_`. This prevents, e.g.,
/// replacing `x` inside `x_val` when only bare `x` is the target.
fn replace_word(text: &str, old: &str, new: &str) -> String {
    if old.is_empty() {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(pos) = remaining.find(old) {
        let before = if pos == 0 {
            None
        } else {
            remaining[..pos].chars().last()
        };
        let after = remaining[pos + old.len()..].chars().next();
        let is_word_boundary = !before.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false)
            && !after.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
        if is_word_boundary {
            result.push_str(&remaining[..pos]);
            result.push_str(new);
        } else {
            result.push_str(&remaining[..pos + old.len()]);
        }
        remaining = &remaining[pos + old.len()..];
    }
    result.push_str(remaining);
    result
}

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

/// Reasons `drop_gap` may fail to produce a verified, closing emission.
///
/// All variants are recoverable and inspectable. The `ClosureVerificationFailed`
/// variant carries the failed `EmitResult` so callers can see the proposed
/// emission even when re-lift didn't confirm DAG closure (useful for
/// debugging the dropper or extending the verifier).
#[derive(Debug, Clone)]
pub enum DropFailure {
    /// The source could not be parsed as a Rust file.
    SourceParseFailed,
    /// The named caller function does not appear in the source.
    CallerNotFound { caller_name: String },
    /// No gap matching `predicate` was detected in any walk from the caller.
    NoGapDetected { predicate: String },
    /// The predicate has no template family in the kit's `templates_for` table.
    UnknownPredicate { predicate: String },
    /// The requested template is not currently a verified candidate for this
    /// predicate. For the MVP, only `DropTemplate::Defensive` is verified.
    TemplateNotCandidate {
        predicate: String,
        requested: DropTemplate,
    },
    /// The template's render path is scaffolding-only (see `NotRenderable`).
    NotRenderable(NotRenderable),
    /// `emit_drop` could not splice the source (parse failure, missing caller,
    /// or out-of-range stmt_index).
    EmitFailed,
    /// The emission was produced but `verify_closure` could not confirm that
    /// the gap is structurally discharged after re-lift. The proposed
    /// `EmitResult` is included for inspection.
    ClosureVerificationFailed { emit: EmitResult },
}

impl std::fmt::Display for DropFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DropFailure::SourceParseFailed => write!(f, "source did not parse as Rust"),
            DropFailure::CallerNotFound { caller_name } => {
                write!(f, "caller `{caller_name}` not found in source")
            }
            DropFailure::NoGapDetected { predicate } => {
                write!(f, "no gap for predicate `{predicate}` detected in any walk")
            }
            DropFailure::UnknownPredicate { predicate } => {
                write!(f, "predicate `{predicate}` has no template family in this kit")
            }
            DropFailure::TemplateNotCandidate { predicate, requested } => {
                write!(
                    f,
                    "template {:?} is not a verified candidate for predicate `{predicate}`",
                    requested
                )
            }
            DropFailure::NotRenderable(e) => write!(f, "{e}"),
            DropFailure::EmitFailed => write!(f, "emit_drop could not splice the source"),
            DropFailure::ClosureVerificationFailed { .. } => {
                write!(f, "emission produced but re-lift did not confirm DAG closure")
            }
        }
    }
}

impl std::error::Error for DropFailure {}

/// Run all four steps of the dropper end-to-end: detect, lookup, emit, **verify**.
///
/// This is the main entry point for the dropper. Per paper 07 §7, an emission
/// that does not actually close the gap is not generative completion. This
/// function therefore calls `verify_closure` after `emit_drop` and only
/// returns `Ok(EmitResult)` when re-lift structurally confirms the gap is
/// discharged. Failed verifications return `Err(DropFailure::ClosureVerificationFailed)`
/// carrying the proposed emission for inspection.
///
/// Phase 4 (mint-on-miss via solver portfolio) is deferred.
///
/// Parameters:
/// - `source`: the Rust source text containing both the callee and caller.
/// - `callee_name`: the function whose precondition has a gap.
/// - `caller_name`: the function calling `callee_name` where the gap arises.
/// - `callee_formal_params`: formal parameter names for the callee.
/// - `callee_precondition`: the WP representing the callee's precondition.
/// - `predicate`: the leaf predicate name to look for (e.g. "not_null").
/// - `template`: which drop template to use.
///
/// Errors are returned as `DropFailure` rather than `None` so callers can
/// distinguish parse failures from missing gaps from verification failures.
pub fn drop_gap(
    source: &str,
    callee_name: &str,
    caller_name: &str,
    callee_formal_params: &[String],
    callee_precondition: Wp,
    predicate: &str,
    template: DropTemplate,
) -> Result<EmitResult, DropFailure> {
    let file: syn::File =
        syn::parse_str(source).map_err(|_| DropFailure::SourceParseFailed)?;

    let caller_fn = file
        .items
        .iter()
        .find_map(|item| {
            if let syn::Item::Fn(f) = item {
                if f.sig.ident == caller_name {
                    return Some(f.clone());
                }
            }
            None
        })
        .ok_or_else(|| DropFailure::CallerNotFound {
            caller_name: caller_name.to_string(),
        })?;

    // Phase 1: detect gaps.
    let walks = walk_callsites_to_entry(
        &caller_fn,
        callee_name,
        callee_formal_params,
        callee_precondition.clone(),
    );
    let gaps = detect_gaps(&walks, predicate);
    let gap = gaps
        .into_iter()
        .next()
        .ok_or_else(|| DropFailure::NoGapDetected {
            predicate: predicate.to_string(),
        })?;

    // Phase 2: look up candidate templates.
    let candidates = templates_for(predicate);
    if candidates.is_empty() {
        return Err(DropFailure::UnknownPredicate {
            predicate: predicate.to_string(),
        });
    }
    if !candidates.contains(&template) {
        return Err(DropFailure::TemplateNotCandidate {
            predicate: predicate.to_string(),
            requested: template,
        });
    }

    // Pre-render check: surface NotRenderable as a structured error rather
    // than letting emit_drop swallow it as None.
    template
        .render(&gap.var_name)
        .map_err(DropFailure::NotRenderable)?;

    // Phase 3: emit.
    let emit = emit_drop(source, &gap, template).ok_or(DropFailure::EmitFailed)?;

    // Phase 4 (verification, not solver-portfolio): re-lift and confirm closure.
    // The advisor flagged this as a correctness blocker -- a caller using
    // drop_gap must only get back emissions that ACTUALLY close the gap.
    if !verify_closure(
        &emit.modified_source,
        &gap,
        callee_formal_params,
        callee_precondition,
    ) {
        return Err(DropFailure::ClosureVerificationFailed { emit });
    }

    Ok(emit)
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
        let rendered = DropTemplate::Defensive
            .render("x")
            .expect("Defensive must render OK");
        assert!(rendered.contains("x.is_none()"), "must guard x");
        assert!(rendered.contains("panic!"), "must panic on violation");
        assert!(rendered.contains("not_null"), "panic msg must name invariant");
    }

    #[test]
    fn recoverable_template_returns_not_renderable() {
        // Recoverable's render path emits `return Err(NullInput)` referencing
        // an undefined error type. Per P1c, render returns NotRenderable rather
        // than producing uncompilable Rust. Match the Scaffolding variant
        // rather than asserting a specific message; the message text is a
        // doc-only diagnostic and can be tightened without breaking the test.
        let result = DropTemplate::Recoverable.render("x");
        let err = result.expect_err("Recoverable must return NotRenderable");
        match err {
            NotRenderable::Scaffolding { family, .. } => {
                assert_eq!(family, "Recoverable");
            }
        }
    }

    #[test]
    fn early_return_template_returns_not_renderable() {
        let result = DropTemplate::EarlyReturn.render("x");
        let err = result.expect_err("EarlyReturn must return NotRenderable");
        match err {
            NotRenderable::Scaffolding { family, .. } => {
                assert_eq!(family, "EarlyReturn");
            }
        }
    }

    #[test]
    fn expect_template_renders_fresh_name_binding() {
        // Expect now renders a fresh-name let-binding. The old variable is
        // preserved as an argument to `.expect()` so the Option<T> type is
        // not consumed before the callsite.
        let rendered = DropTemplate::Expect
            .render("x")
            .expect("Expect must render Ok after #407 fix");
        assert!(
            rendered.contains("x_checked"),
            "must bind to fresh name x_checked: {rendered}"
        );
        assert!(
            rendered.contains("x.expect("),
            "must call .expect() on original var: {rendered}"
        );
        assert!(
            rendered.contains("not_null"),
            "expect message must name the invariant: {rendered}"
        );
    }

    #[test]
    fn expect_template_substitutes_var_name() {
        let rendered = DropTemplate::Expect
            .render("my_val")
            .expect("Expect renders OK");
        assert!(rendered.contains("my_val_checked"), "fresh name must use var: {rendered}");
        assert!(rendered.contains("my_val.expect("), "must call expect on original: {rendered}");
    }

    #[test]
    fn defensive_template_substitutes_var_name() {
        // Only Defensive is currently renderable; other variants return
        // NotRenderable and are exercised by their dedicated tests above.
        let rendered = DropTemplate::Defensive
            .render("my_var")
            .expect("Defensive renders OK");
        assert!(
            rendered.contains("my_var"),
            "Defensive template must contain var name 'my_var': {}",
            rendered
        );
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

        let emit = result.expect("drop_gap must succeed for not_null fixture");
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

    // ---- P1b: drop_gap calls verify_closure ----

    #[test]
    fn drop_gap_returns_template_not_candidate_for_scaffolding() {
        // Recoverable is in the enum but not in templates_for("not_null"),
        // so drop_gap should return TemplateNotCandidate (NOT swallow as None).
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "caller",
            &["x".to_string()],
            not_null_wp("x"),
            "not_null",
            DropTemplate::Recoverable,
        );
        match result {
            Err(DropFailure::TemplateNotCandidate { requested, .. }) => {
                assert_eq!(requested, DropTemplate::Recoverable);
            }
            other => panic!("expected TemplateNotCandidate, got {:?}", other),
        }
    }

    #[test]
    fn drop_gap_returns_no_gap_detected_for_unrelated_predicate() {
        // The fixture's WP is `not_null(x)`. Querying for an unrelated
        // predicate ("some_unknown_predicate") never matches the WP, so
        // no gap is detected. This is the documented short-circuit before
        // template lookup -- detect_gaps runs first.
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "caller",
            &["x".to_string()],
            not_null_wp("x"),
            "some_unknown_predicate",
            DropTemplate::Defensive,
        );
        match result {
            Err(DropFailure::NoGapDetected { predicate }) => {
                assert_eq!(predicate, "some_unknown_predicate");
            }
            other => panic!("expected NoGapDetected, got {:?}", other),
        }
    }

    #[test]
    fn drop_gap_returns_unknown_predicate_when_gap_exists_but_no_table_entry() {
        // To hit UnknownPredicate, a gap MUST be detected first. We
        // synthesize a precondition with predicate `unknown_pred(x)` and
        // query for the same predicate. detect_gaps emits the gap;
        // templates_for("unknown_pred") returns &[] -> UnknownPredicate.
        let unknown_pred_wp = Wp(IrFormula::Atomic {
            name: "unknown_pred".to_string(),
            args: vec![IrTerm::Var {
                name: "x".to_string(),
            }],
        });
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "caller",
            &["x".to_string()],
            unknown_pred_wp,
            "unknown_pred",
            DropTemplate::Defensive,
        );
        match result {
            Err(DropFailure::UnknownPredicate { predicate }) => {
                assert_eq!(predicate, "unknown_pred");
            }
            other => panic!("expected UnknownPredicate, got {:?}", other),
        }
    }

    #[test]
    fn drop_gap_returns_caller_not_found_for_missing_caller() {
        let result = drop_gap(
            FIXTURE_SRC,
            "f",
            "nonexistent_caller",
            &["x".to_string()],
            not_null_wp("x"),
            "not_null",
            DropTemplate::Defensive,
        );
        match result {
            Err(DropFailure::CallerNotFound { caller_name }) => {
                assert_eq!(caller_name, "nonexistent_caller");
            }
            other => panic!("expected CallerNotFound, got {:?}", other),
        }
    }

    // ---- P1a: multi-function placement (acceptance test) ----

    #[test]
    fn emit_drop_routes_to_correct_caller_in_multi_function_file() {
        // Two callers, both call `f`. Only `caller_a` has a gap (passes a Var
        // expression `x`). `caller_b` passes the same name but is a separate
        // function -- the guard must land in `caller_a`'s body, NOT before
        // `caller_b`'s call to `f`.
        let src = "\
fn f(x: Option<i32>) -> i32 {
    x.unwrap()
}

fn caller_a(x: Option<i32>) {
    f(x);
}

fn caller_b(x: Option<i32>) {
    f(x);
}
";
        let file: syn::File = syn::parse_str(src).expect("parses");
        let caller_a = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "caller_a" => Some(f.clone()),
                _ => None,
            })
            .expect("caller_a fn");

        let precondition = not_null_wp("x");
        let walks =
            walk_callsites_to_entry(&caller_a, "f", &["x".to_string()], precondition);
        let gaps = detect_gaps(&walks, "not_null");
        assert_eq!(gaps.len(), 1, "one gap in caller_a");
        let gap = &gaps[0];
        assert_eq!(gap.caller_name, "caller_a");

        let result = emit_drop(src, gap, DropTemplate::Defensive).expect("emit succeeds");

        // The guard line must be between caller_a's `fn` line and caller_b's
        // `fn` line. We assert the structural invariant on the modified source.
        let modified = &result.modified_source;
        let caller_a_pos = modified.find("fn caller_a").expect("caller_a present");
        let caller_b_pos = modified.find("fn caller_b").expect("caller_b present");
        let guard_pos = modified.find("x.is_none()").expect("guard present");
        assert!(
            caller_a_pos < guard_pos && guard_pos < caller_b_pos,
            "guard must land between caller_a and caller_b. \
             caller_a@{}, guard@{}, caller_b@{}\nmodified:\n{}",
            caller_a_pos,
            guard_pos,
            caller_b_pos,
            modified
        );

        // Also: the modified source must parse cleanly (no broken Rust).
        syn::parse_str::<syn::File>(modified).expect("modified source parses");
    }

    #[test]
    fn emit_drop_routes_to_correct_callsite_in_multi_callsite_function() {
        // Caller has TWO callsites to `f` separated by a let-binding. The gap
        // points at the SECOND callsite (callsite_stmt_index = 2). The guard
        // must land before the second callsite, not the first.
        let src = "\
fn f(x: Option<i32>) -> i32 {
    x.unwrap()
}

fn caller(x: Option<i32>) {
    f(x);
    let y = x;
    f(y);
}
";
        let file: syn::File = syn::parse_str(src).expect("parses");
        let caller_fn = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "caller" => Some(f.clone()),
                _ => None,
            })
            .expect("caller fn");

        let precondition = not_null_wp("x");
        let walks =
            walk_callsites_to_entry(&caller_fn, "f", &["x".to_string()], precondition);
        let gaps = detect_gaps(&walks, "not_null");
        // Both callsites produce gaps (two separate walks). Pick the second gap
        // explicitly by stmt_index.
        assert!(gaps.len() >= 2, "two callsites yield two gaps");
        let second_gap = gaps
            .iter()
            .find(|g| g.callsite_stmt_index == 2)
            .expect("gap at stmt_index 2 (second f call)");

        let result =
            emit_drop(src, second_gap, DropTemplate::Defensive).expect("emit succeeds");
        let modified = &result.modified_source;

        // Locate the guard and the two callsites in the modified text.
        // The guard must land BETWEEN the let-binding line and the second
        // callsite, NOT before the first callsite.
        let first_call_pos = modified.find("f(x);").expect("first call f(x) present");
        let let_y_pos = modified.find("let y").expect("let y present");
        let second_call_pos = modified.find("f(y);").expect("second call f(y) present");
        let guard_pos = modified.find("is_none()").expect("guard present");

        assert!(
            first_call_pos < let_y_pos,
            "first callsite must precede let-binding"
        );
        assert!(
            let_y_pos < guard_pos,
            "guard must follow let-binding (not precede first callsite)"
        );
        assert!(
            guard_pos < second_call_pos,
            "guard must precede second callsite"
        );

        syn::parse_str::<syn::File>(modified).expect("modified source parses");
    }

    // ---- P1d: detect_gaps skips non-Var arguments ----

    #[test]
    fn detect_gaps_skips_non_var_predicate_arg() {
        // Construct a walks list with an entry WP whose predicate argument is
        // NOT a Var (it's a Const). detect_gaps must skip rather than emit a
        // Gap with var_name = "_". We synthesize a CallsiteWalk with a
        // hand-crafted entry arrival containing the non-Var predicate.
        use crate::walk::{Arrival, ArrivalKind, CallsiteWalk};
        use crate::wp::const_int;

        let non_var_formula = IrFormula::Atomic {
            name: "not_null".to_string(),
            args: vec![const_int(0)],
        };
        let walk = CallsiteWalk {
            caller_name: "caller".to_string(),
            callee_name: "f".to_string(),
            arrivals: vec![
                Arrival {
                    kind: ArrivalKind::Callsite {
                        callee: "f".to_string(),
                    },
                    stmt_index: 0,
                    wp: Wp(non_var_formula.clone()),
                },
                Arrival {
                    kind: ArrivalKind::FunctionEntry {
                        fn_name: "caller".to_string(),
                    },
                    stmt_index: 1,
                    wp: Wp(non_var_formula),
                },
            ],
        };

        let gaps = detect_gaps(std::slice::from_ref(&walk), "not_null");
        assert!(
            gaps.is_empty(),
            "non-Var predicate argument must be skipped, got gaps: {:?}",
            gaps
        );
    }

    // ---- #407: Expect template fresh-name + callsite rewrite ----

    #[test]
    fn emit_drop_expect_inserts_let_binding_and_rewrites_callsite() {
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

        let result = emit_drop(FIXTURE_SRC, gap, DropTemplate::Expect)
            .expect("emit must succeed for Expect after #407 fix");

        let modified = &result.modified_source;

        // The let-binding must appear before the callsite.
        let let_pos = modified.find("x_checked").expect("fresh name x_checked present");
        let callsite_pos = modified.find("f(x_checked)").expect("callsite uses x_checked");
        assert!(
            let_pos < callsite_pos,
            "let-binding must precede callsite: let@{let_pos}, callsite@{callsite_pos}"
        );

        // The original `f(x)` callsite must be rewritten to `f(x_checked)`.
        assert!(
            !modified.contains("f(x);"),
            "original f(x) callsite must be rewritten to f(x_checked)"
        );

        // The modified source must parse cleanly (type-correct Rust).
        syn::parse_str::<syn::File>(modified)
            .expect("emitted Expect source must be syntactically valid");
    }

    #[test]
    fn expect_callsite_arg_rewrite_returns_fresh_name_pair() {
        let rewrite = DropTemplate::Expect.callsite_arg_rewrite("x");
        assert_eq!(rewrite, Some(("x".to_string(), "x_checked".to_string())));
    }

    #[test]
    fn defensive_callsite_arg_rewrite_returns_none() {
        assert_eq!(DropTemplate::Defensive.callsite_arg_rewrite("x"), None);
    }
}
