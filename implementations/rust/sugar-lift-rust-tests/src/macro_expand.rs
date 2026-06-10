//! A correctness-first `macro_rules!` expander for the assertion lifter.
//!
//! The lifter desugars a language using the language: `a(x)` desugars to `a(x)`,
//! then we walk into the definition of `a`. For a `macro_rules!` macro, walking
//! into the definition means expanding it. This module performs that expansion
//! for the matcher shapes it can match EXACTLY, and refuses (returns
//! `Err(reason)`) for anything it cannot match. It never guesses an expansion:
//! a wrong expansion would be a false-pass, so unsupported matcher grammar is a
//! named refusal, not a silent or approximate result.
//!
//! Supported matcher grammar (the common assertion-macro shapes):
//!   - literal tokens (idents, puncts, literals, nested delimiter groups)
//!   - single-fragment metavariables: `$x:expr|ty|ident|literal|tt|pat|path|block`
//!   - one optional group: `$( ... )?`
//!   - one repetition group: `$( ... )sep*` / `$( ... )sep+` (separator optional)
//! Nested repetitions, `::+`-style segment repetition, and other advanced
//! grammar return `Err` so the caller refuses by name.

use std::collections::BTreeMap;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

/// One `(matcher) => { body }` arm of a `macro_rules!` definition.
pub(crate) struct MacroRule {
    matcher: Vec<TokenTree>,
    body: TokenStream,
}

/// A captured metavariable binding.
#[derive(Clone)]
enum Binding {
    /// A single fragment capture: the tokens bound to `$x`.
    Single(TokenStream),
    /// A repetition capture: one set of inner bindings per repetition round.
    Repeated(Vec<Bindings>),
}

type Bindings = BTreeMap<String, Binding>;

/// Parse the token stream of a `macro_rules!` body into its rules.
/// The grammar is `(matcher) => { body } ;` repeated. Returns `Err` if the
/// shape is not recognized so the caller refuses rather than mis-parses.
pub(crate) fn parse_rules(tokens: TokenStream) -> Result<Vec<MacroRule>, String> {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut rules = Vec::new();
    let mut i = 0;
    while i < trees.len() {
        // matcher group
        let matcher = match &trees[i] {
            TokenTree::Group(g) => g.stream().into_iter().collect::<Vec<_>>(),
            other => return Err(format!("macro_rules: expected matcher group, got {other}")),
        };
        i += 1;
        // `=>`
        let arrow_ok = matches!(trees.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '=' && p.spacing() == Spacing::Joint)
            && matches!(trees.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == '>');
        if !arrow_ok {
            return Err("macro_rules: expected `=>` after matcher".to_string());
        }
        i += 2;
        // body group
        let body = match trees.get(i) {
            Some(TokenTree::Group(g)) => g.stream(),
            other => return Err(format!("macro_rules: expected body group, got {other:?}")),
        };
        i += 1;
        rules.push(MacroRule { matcher, body });
        // optional `;` separator
        if matches!(trees.get(i), Some(TokenTree::Punct(p)) if p.as_char() == ';') {
            i += 1;
        }
    }
    if rules.is_empty() {
        return Err("macro_rules: no rules parsed".to_string());
    }
    Ok(rules)
}

/// A stable textual signature of a rule set, used to tell whether two scanned
/// definitions of the same macro name are the same definition (the crate seen
/// twice) or genuinely conflicting (ambiguous).
pub(crate) fn rules_signature(rules: &[MacroRule]) -> String {
    rules
        .iter()
        .map(|r| {
            let matcher: String = r
                .matcher
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            format!("{matcher} => {{ {} }}", r.body)
        })
        .collect::<Vec<_>>()
        .join(" ; ")
}

/// Expand an invocation `input` against a macro's rules. Tries each rule in
/// order; the first whose matcher matches the entire input is transcribed.
/// Returns `Err` if no rule matches or the matched rule uses grammar the
/// transcriber does not support.
pub(crate) fn expand(rules: &[MacroRule], input: TokenStream) -> Result<TokenStream, String> {
    let input_trees: Vec<TokenTree> = input.into_iter().collect();
    for rule in rules {
        if let Some(bindings) = match_seq(&rule.matcher, &input_trees) {
            return transcribe(rule.body.clone(), &bindings);
        }
    }
    Err("macro expansion: no rule matched the invocation".to_string())
}

/// Match a matcher token sequence against the full input sequence. Returns the
/// captured bindings only on a complete match (matcher and input both consumed).
fn match_seq(matcher: &[TokenTree], input: &[TokenTree]) -> Option<Bindings> {
    let mut bindings = Bindings::new();
    let consumed = match_prefix(matcher, input, &mut bindings, None)?;
    if consumed == input.len() {
        Some(bindings)
    } else {
        None
    }
}

/// Try to match `matcher` against a prefix of `input`, recording bindings.
/// Returns the number of input token-trees consumed, or `None` on mismatch /
/// unsupported grammar.
fn match_prefix(
    matcher: &[TokenTree],
    input: &[TokenTree],
    bindings: &mut Bindings,
    terminator: Option<&TokenTree>,
) -> Option<usize> {
    let mut mi = 0;
    let mut ii = 0;
    while mi < matcher.len() {
        match &matcher[mi] {
            // Metavariable or repetition: `$`
            TokenTree::Punct(p) if p.as_char() == '$' => {
                match matcher.get(mi + 1) {
                    // `$( ... )...` repetition or optional group
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                        // separator and repeat operator follow the group
                        let (sep, op, advance) = parse_rep_suffix(&matcher[mi + 2..])?;
                        let follow = &matcher[mi + 2 + advance..];
                        let consumed =
                            match_repetition(&inner, sep, op, follow, &input[ii..], bindings)?;
                        ii += consumed;
                        mi += 2 + advance;
                    }
                    // `$name:frag` metavariable
                    Some(TokenTree::Ident(name)) => {
                        // expect `:` then fragment specifier ident
                        let colon_ok = matches!(matcher.get(mi + 2), Some(TokenTree::Punct(p)) if p.as_char() == ':');
                        let frag = match matcher.get(mi + 3) {
                            Some(TokenTree::Ident(f)) if colon_ok => f.to_string(),
                            _ => return None, // unsupported: `$name` without fragment spec
                        };
                        // Follow token for a greedy fragment: the next matcher
                        // token, or the caller's terminator (e.g. a repetition
                        // separator) when this metavar ends the matcher.
                        let follow = matcher.get(mi + 4).or(terminator);
                        let (captured, consumed) = capture_fragment(&frag, &input[ii..], follow)?;
                        bindings.insert(name.to_string(), Binding::Single(captured));
                        ii += consumed;
                        mi += 4;
                    }
                    _ => return None,
                }
            }
            // Literal matcher token: must equal the next input token-tree.
            m => {
                let inp = input.get(ii)?;
                if !token_tree_eq(m, inp) {
                    return None;
                }
                ii += 1;
                mi += 1;
            }
        }
    }
    Some(ii)
}

/// Parse the `sep? (*|+|?)` suffix that follows a `$( ... )` group.
/// Returns (separator token, operator char, number of matcher tokens consumed).
fn parse_rep_suffix(rest: &[TokenTree]) -> Option<(Option<TokenTree>, char, usize)> {
    match rest.first() {
        Some(TokenTree::Punct(p)) if matches!(p.as_char(), '*' | '+' | '?') => {
            Some((None, p.as_char(), 1))
        }
        // a separator token then the operator
        Some(sep) => match rest.get(1) {
            Some(TokenTree::Punct(p)) if matches!(p.as_char(), '*' | '+' | '?') => {
                Some((Some(sep.clone()), p.as_char(), 2))
            }
            _ => None,
        },
        None => None,
    }
}

/// Match a repetition group against input, stopping when the `follow` tokens
/// would match or input is exhausted. Records repeated inner bindings.
fn match_repetition(
    inner: &[TokenTree],
    sep: Option<TokenTree>,
    op: char,
    follow: &[TokenTree],
    input: &[TokenTree],
    bindings: &mut Bindings,
) -> Option<usize> {
    let mut ii = 0;
    let mut rounds: Vec<Bindings> = Vec::new();
    loop {
        // For `?` at most one round; stop if we already did one.
        if op == '?' && !rounds.is_empty() {
            break;
        }
        // If the follow tokens match here, stop the repetition.
        if !follow.is_empty() && follow_matches(follow, &input[ii..]) {
            break;
        }
        if ii >= input.len() {
            break;
        }
        // Expect a separator before all but the first round.
        if let (Some(sep_tok), false) = (&sep, rounds.is_empty()) {
            match input.get(ii) {
                Some(t) if token_tree_eq(sep_tok, t) => ii += 1,
                _ => break,
            }
        }
        let mut round = Bindings::new();
        // Inside a round, a trailing greedy fragment must stop at the separator
        // (if any) or at the repetition's follow token.
        let round_terminator = sep.as_ref().or_else(|| follow.first());
        let consumed = match_prefix(inner, &input[ii..], &mut round, round_terminator)?;
        if consumed == 0 {
            break;
        }
        ii += consumed;
        rounds.push(round);
    }
    if op == '+' && rounds.is_empty() {
        return None;
    }
    // Merge: each metavar named in `inner` becomes a Repeated binding.
    for name in metavar_names(inner) {
        let collected = rounds
            .iter()
            .map(|r| {
                let mut m = Bindings::new();
                if let Some(b) = r.get(&name) {
                    m.insert(name.clone(), b.clone());
                }
                m
            })
            .collect();
        bindings.insert(name, Binding::Repeated(collected));
    }
    Some(ii)
}

/// Does the follow-sequence match at the start of `input`? Used to terminate a
/// repetition. Only checks literal lookahead (one token) which suffices for the
/// supported shapes; a metavar follow conservatively does not terminate.
fn follow_matches(follow: &[TokenTree], input: &[TokenTree]) -> bool {
    match (follow.first(), input.first()) {
        (Some(f @ TokenTree::Punct(_)), Some(i)) => token_tree_eq(f, i),
        (Some(f @ TokenTree::Ident(_)), Some(i)) => token_tree_eq(f, i),
        (Some(f @ TokenTree::Literal(_)), Some(i)) => token_tree_eq(f, i),
        _ => false,
    }
}

/// Capture one fragment of the given specifier from the start of `input`.
/// `expr`/`ty`/`pat`/`path`/`block` capture greedily up to the follow token (or
/// end); `ident`/`literal`/`tt` capture exactly one token-tree. The captured
/// tokens are validated by re-parsing for `expr` to avoid binding garbage.
fn capture_fragment(
    frag: &str,
    input: &[TokenTree],
    follow: Option<&TokenTree>,
) -> Option<(TokenStream, usize)> {
    match frag {
        "ident" => match input.first() {
            Some(t @ TokenTree::Ident(_)) => Some((std::iter::once(t.clone()).collect(), 1)),
            _ => None,
        },
        "literal" => match input.first() {
            Some(t @ TokenTree::Literal(_)) => Some((std::iter::once(t.clone()).collect(), 1)),
            _ => None,
        },
        "tt" => input
            .first()
            .map(|t| (std::iter::once(t.clone()).collect(), 1)),
        // Greedy fragments: consume token-trees until the follow token matches
        // or input ends. Validate `expr` by re-parsing.
        "expr" | "ty" | "pat" | "path" | "block" => {
            let mut n = 0;
            while n < input.len() {
                if let Some(f) = follow {
                    if token_tree_eq(f, &input[n]) {
                        break;
                    }
                }
                n += 1;
            }
            if n == 0 {
                return None;
            }
            let captured: TokenStream = input[..n].iter().cloned().collect();
            if frag == "expr" && syn::parse2::<syn::Expr>(captured.clone()).is_err() {
                return None;
            }
            Some((captured, n))
        }
        _ => None,
    }
}

/// Transcribe a macro body, substituting `$name` with bound tokens and expanding
/// `$( ... )...` repetition groups using the repeated bindings.
fn transcribe(body: TokenStream, bindings: &Bindings) -> Result<TokenStream, String> {
    let trees: Vec<TokenTree> = body.into_iter().collect();
    let mut out = TokenStream::new();
    let mut i = 0;
    while i < trees.len() {
        match &trees[i] {
            TokenTree::Punct(p) if p.as_char() == '$' => match trees.get(i + 1) {
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                    let inner = g.stream();
                    let (sep, _op, advance) = parse_rep_suffix(&trees[i + 2..])
                        .ok_or("transcribe: malformed repetition suffix")?;
                    let rounds = repetition_round_count(&inner, bindings)?;
                    for r in 0..rounds {
                        if r > 0 {
                            if let Some(s) = &sep {
                                out.extend(std::iter::once(s.clone()));
                            }
                        }
                        let round_bindings = project_round(bindings, r);
                        out.extend(transcribe(inner.clone(), &round_bindings)?);
                    }
                    i += 2 + advance;
                }
                Some(TokenTree::Ident(name)) => {
                    match bindings.get(&name.to_string()) {
                        Some(Binding::Single(ts)) => out.extend(ts.clone()),
                        Some(Binding::Repeated(_)) => {
                            return Err(format!(
                                "transcribe: `${name}` used outside its repetition"
                            ))
                        }
                        None => return Err(format!("transcribe: unbound metavariable `${name}`")),
                    }
                    i += 2;
                }
                _ => return Err("transcribe: unsupported `$` usage".to_string()),
            },
            TokenTree::Group(g) => {
                // Recurse into nested groups, preserving the delimiter.
                let inner = transcribe(g.stream(), bindings)?;
                out.extend(std::iter::once(TokenTree::Group(proc_macro2::Group::new(
                    g.delimiter(),
                    inner,
                ))));
                i += 1;
            }
            other => {
                out.extend(std::iter::once(other.clone()));
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Number of repetition rounds for a `$( ... )` transcriber group: the length of
/// the first repeated binding referenced inside it.
fn repetition_round_count(inner: &TokenStream, bindings: &Bindings) -> Result<usize, String> {
    for name in metavar_names(&inner.clone().into_iter().collect::<Vec<_>>()) {
        if let Some(Binding::Repeated(rounds)) = bindings.get(&name) {
            return Ok(rounds.len());
        }
    }
    Err("transcribe: repetition group references no repeated metavariable".to_string())
}

/// Project the r-th round of every repeated binding to a flat binding set.
fn project_round(bindings: &Bindings, r: usize) -> Bindings {
    let mut out = Bindings::new();
    for (name, b) in bindings {
        match b {
            Binding::Repeated(rounds) => {
                if let Some(round) = rounds.get(r) {
                    if let Some(inner) = round.get(name) {
                        out.insert(name.clone(), inner.clone());
                    }
                }
            }
            Binding::Single(_) => {
                out.insert(name.clone(), b.clone());
            }
        }
    }
    out
}

/// Names of metavariables (`$name:frag`) appearing in a matcher token sequence.
fn metavar_names(matcher: &[TokenTree]) -> Vec<String> {
    let mut names = Vec::new();
    let mut i = 0;
    while i < matcher.len() {
        match &matcher[i] {
            TokenTree::Punct(p) if p.as_char() == '$' => match matcher.get(i + 1) {
                Some(TokenTree::Ident(name)) => {
                    names.push(name.to_string());
                    i += 2;
                }
                Some(TokenTree::Group(g)) => {
                    names.extend(metavar_names(&g.stream().into_iter().collect::<Vec<_>>()));
                    i += 2;
                }
                _ => i += 1,
            },
            // Recurse into every delimiter group so nested `$e` (e.g. inside
            // `assert_eq!($e, 0)`) is found, not only `$( ... )` groups.
            TokenTree::Group(g) => {
                names.extend(metavar_names(&g.stream().into_iter().collect::<Vec<_>>()));
                i += 1;
            }
            _ => i += 1,
        }
    }
    names
}

/// Structural token-tree equality (delimiter + recursive stream, or token text).
fn token_tree_eq(a: &TokenTree, b: &TokenTree) -> bool {
    match (a, b) {
        (TokenTree::Ident(x), TokenTree::Ident(y)) => x == y,
        (TokenTree::Punct(x), TokenTree::Punct(y)) => x.as_char() == y.as_char(),
        (TokenTree::Literal(x), TokenTree::Literal(y)) => x.to_string() == y.to_string(),
        (TokenTree::Group(x), TokenTree::Group(y)) => {
            x.delimiter() == y.delimiter() && {
                let xs: Vec<TokenTree> = x.stream().into_iter().collect();
                let ys: Vec<TokenTree> = y.stream().into_iter().collect();
                xs.len() == ys.len() && xs.iter().zip(&ys).all(|(p, q)| token_tree_eq(p, q))
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn rules_of(def: TokenStream) -> Vec<MacroRule> {
        parse_rules(def).expect("rules parse")
    }

    #[test]
    fn expands_single_expr_wrapper() {
        // assert_ok!($e:expr) => { assert!($e.is_ok()) }
        let def = quote! { ($e:expr) => { assert!($e.is_ok()) }; };
        let out = expand(&rules_of(def), quote! { foo(x) }).expect("expand");
        assert_eq!(
            out.to_string(),
            quote! { assert!(foo(x).is_ok()) }.to_string()
        );
    }

    #[test]
    fn expands_typed_two_expr_const_safe_shape() {
        // assert_eq_const_safe!($t:ty: $l:expr, $r:expr) => { assert_eq!($l, $r) }
        let def = quote! { ($t:ty: $l:expr, $r:expr) => { assert_eq!($l, $r) }; };
        let out = expand(&rules_of(def), quote! { u8: make(), 42 }).expect("expand");
        assert_eq!(
            out.to_string(),
            quote! { assert_eq!(make(), 42) }.to_string()
        );
    }

    #[test]
    fn picks_correct_rule_by_literal_token() {
        let def = quote! {
            (ok, $e:expr) => { assert!($e.is_ok()) };
            (err, $e:expr) => { assert!($e.is_err()) };
        };
        let out = expand(&rules_of(def), quote! { err, foo(x) }).expect("expand");
        assert_eq!(
            out.to_string(),
            quote! { assert!(foo(x).is_err()) }.to_string()
        );
    }

    #[test]
    fn expands_repetition_with_separator() {
        // all_eq!($($e:expr),*) => { $( assert_eq!($e, 0); )* }
        let def = quote! { ($($e:expr),*) => { $( assert_eq!($e, 0); )* }; };
        let res = expand(&rules_of(def), quote! { a, b, c });
        let out = res.unwrap_or_else(|e| panic!("expand err: {e}"));
        assert_eq!(
            out.to_string(),
            quote! { assert_eq!(a, 0); assert_eq!(b, 0); assert_eq!(c, 0); }.to_string()
        );
    }

    #[test]
    fn refuses_when_no_rule_matches() {
        let def = quote! { (ok, $e:expr) => { assert!($e.is_ok()) }; };
        assert!(expand(&rules_of(def), quote! { nope }).is_err());
    }

    #[test]
    fn refuses_garbage_expr_capture() {
        // `$e:expr` followed by end; input that is not a valid expr must not bind.
        let def = quote! { ($e:expr) => { assert!($e) }; };
        // `+ +` is not a valid expression.
        assert!(expand(&rules_of(def), quote! { + + }).is_err());
    }
}
