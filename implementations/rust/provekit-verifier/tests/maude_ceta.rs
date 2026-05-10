use provekit_verifier::solvers::ceta::{parse_ceta_output, CetaDecision};
use provekit_verifier::solvers::maude::{parse_maude_output, MaudeDecision};

#[test]
fn maude_parser_accepts_equal_reduce_normal_forms() {
    let stdout = "\
reduce in PROVEKIT-NAT : plus(s(zero), s(zero)) .
result Nat: s(s(zero))
reduce in PROVEKIT-NAT : s(s(zero)) .
result Nat: s(s(zero))
search in PROVEKIT-NAT : plus(s(zero), s(zero)) =>* s(s(zero)) .
No solution.
";
    let parsed = parse_maude_output(stdout).unwrap();
    assert_eq!(parsed.normal_forms, vec!["s(s(zero))", "s(s(zero))"]);
    assert_eq!(parsed.decision, MaudeDecision::ReduceEqual);
}

#[test]
fn maude_parser_accepts_search_solution() {
    let stdout = "\
reduce in M : lhs .
result Elt: lhs
reduce in M : rhs .
result Elt: rhs
search in M : lhs =>* rhs .
Solution 1
state: rhs
";
    let parsed = parse_maude_output(stdout).unwrap();
    assert_eq!(parsed.decision, MaudeDecision::SearchSolution);
}

#[test]
fn maude_parser_returns_unknown_for_no_match() {
    let stdout = "\
reduce in M : lhs .
result Elt: lhs
reduce in M : rhs .
result Elt: rhs
search in M : lhs =>* rhs .
No solution.
";
    let parsed = parse_maude_output(stdout).unwrap();
    assert_eq!(parsed.decision, MaudeDecision::NoMatch);
}

#[test]
fn ceta_parser_accepts_only_certified_success() {
    assert_eq!(parse_ceta_output("YES\n").unwrap(), CetaDecision::Accepted);
    assert_eq!(
        parse_ceta_output("Certificate accepted\n").unwrap(),
        CetaDecision::Accepted
    );
    assert_eq!(parse_ceta_output("NO\n").unwrap(), CetaDecision::Rejected);
    assert_eq!(
        parse_ceta_output("error: invalid certificate\n").unwrap(),
        CetaDecision::Rejected
    );
}

#[test]
fn non_confluent_gate_rejection_discards_reduce() {
    let maude_stdout = "\
reduce in BAD : a .
result Elt: c
reduce in BAD : b .
result Elt: c
search in BAD : a =>* b .
No solution.
";
    let maude = parse_maude_output(maude_stdout).unwrap();
    assert_eq!(maude.decision, MaudeDecision::ReduceEqual);
    let ceta = parse_ceta_output("NO\n").unwrap();
    assert_eq!(ceta, CetaDecision::Rejected);
}

#[test]
#[ignore = "requires maude, termination provers, confluence checker, and ceta on PATH"]
fn binary_dependent_maude_and_ceta_gate_smoke() {
    panic!("enable locally after installing the Maude and CeTA portfolio tools");
}
