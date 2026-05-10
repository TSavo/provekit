// SPDX-License-Identifier: Apache-2.0
//
// Rust language signature lookup.
//
// The CIDs are minted in menagerie/rust-language-signature. This module
// embeds the component list so lifted Rust terms can resolve surface
// constructors into the rust:rust operation address space.

use std::collections::HashMap;
use std::sync::OnceLock;

pub const RUST_LANGUAGE_SIGNATURE_CID: &str = "blake3-512:bfceb0d839ea1de168647faa9cf0ee128d6205333d7388122992cd2a7ba39481bc7fdc4a8d1c107ee6c103fe7bb5174f96801f584db1fcb7b66ef79c3c4b7354";

const COMPONENT_CIDS_JSON: &str =
    include_str!("../../../../menagerie/rust-language-signature/component-cids.json");

static OP_CIDS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

pub fn op_cid(surface_name: &str) -> Option<&'static str> {
    let op_name = canonical_operation_name(surface_name)?;
    OP_CIDS.get_or_init(load_op_cids).get(op_name).copied()
}

pub fn canonical_operation_name(surface_name: &str) -> Option<&str> {
    match surface_name {
        "==" | "=" | "eq" => Some("eq"),
        "!=" | "ne" => Some("ne"),
        "\u{2260}" => Some("ne"),
        "+" | "add" => Some("add"),
        "-" | "sub" => Some("sub"),
        "*" | "mul" => Some("mul"),
        "/" | "div" => Some("div"),
        "%" | "rem" => Some("rem"),
        "<" | "lt" => Some("lt"),
        "<=" | "le" => Some("le"),
        "\u{2264}" => Some("le"),
        ">" | "gt" => Some("gt"),
        ">=" | "ge" => Some("ge"),
        "\u{2265}" => Some("ge"),
        "&&" | "and" => Some("and"),
        "||" | "or" => Some("or"),
        "!" | "not" => Some("not"),
        "&" | "bit_and" => Some("bit_and"),
        "|" | "bit_or" => Some("bit_or"),
        "^" | "bit_xor" => Some("bit_xor"),
        "<<" | "shl" => Some("shl"),
        ">>" | "shr" => Some("shr"),
        "bit-not" | "bit_not" => Some("bit_not"),
        "field" => Some("field"),
        name if name.starts_with("method:") => Some("method_call"),
        name if name.starts_with("call:") => Some("call_result"),
        op @ ("skip" | "seq" | "if" | "while" | "for" | "switch" | "call" | "return" | "break"
        | "continue" | "deref" | "member" | "assign" | "neg" | "panic" | "try"
        | "try_option" | "match" | "match_expr" | "borrow" | "borrow_mut" | "deref_raw"
        | "drop" | "await" | "index" | "box_new" | "closure" | "closure_call" | "cast"
        | "move" | "range" | "range_incl" | "tuple" | "array" | "array_repeat" | "len"
        | "ite" | "method_call" | "call_result" | "loop" | "let" | "into_iter" | "next"
        | "arm" | "arms" | "guarded_arm" | "if_let" | "while_let" | "pattern_ok"
        | "pattern_err" | "pattern_some" | "pattern_none" | "pattern_wild"
        | "pattern_bind") => Some(op),
        _ => None,
    }
}

fn load_op_cids() -> HashMap<&'static str, &'static str> {
    let rows: serde_json::Value =
        serde_json::from_str(COMPONENT_CIDS_JSON).expect("embedded component-cids.json is valid");
    let mut by_spec = HashMap::new();
    let Some(items) = rows.as_array() else {
        return by_spec;
    };
    for item in items {
        if item.get("kind").and_then(|v| v.as_str()) != Some("algorithm") {
            continue;
        }
        let Some(spec) = item.get("spec").and_then(|v| v.as_str()) else {
            continue;
        };
        if !spec.starts_with("op_") {
            continue;
        }
        let Some(cid) = item.get("cid").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(op_name) = op_name_from_spec(spec) else {
            continue;
        };
        by_spec.insert(
            op_name,
            Box::leak(cid.to_string().into_boxed_str()) as &'static str,
        );
    }
    by_spec
}

fn op_name_from_spec(spec: &str) -> Option<&'static str> {
    match spec {
        "op_add.spec.json" => Some("add"),
        "op_and.spec.json" => Some("and"),
        "op_arm.spec.json" => Some("arm"),
        "op_arms.spec.json" => Some("arms"),
        "op_array.spec.json" => Some("array"),
        "op_array_repeat.spec.json" => Some("array_repeat"),
        "op_assign.spec.json" => Some("assign"),
        "op_await.spec.json" => Some("await"),
        "op_bit_and.spec.json" => Some("bit_and"),
        "op_bit_not.spec.json" => Some("bit_not"),
        "op_bit_or.spec.json" => Some("bit_or"),
        "op_bit_xor.spec.json" => Some("bit_xor"),
        "op_borrow.spec.json" => Some("borrow"),
        "op_borrow_mut.spec.json" => Some("borrow_mut"),
        "op_box_new.spec.json" => Some("box_new"),
        "op_break.spec.json" => Some("break"),
        "op_call.spec.json" => Some("call"),
        "op_call_result.spec.json" => Some("call_result"),
        "op_cast.spec.json" => Some("cast"),
        "op_closure.spec.json" => Some("closure"),
        "op_closure_call.spec.json" => Some("closure_call"),
        "op_continue.spec.json" => Some("continue"),
        "op_deref.spec.json" => Some("deref"),
        "op_deref_raw.spec.json" => Some("deref_raw"),
        "op_div.spec.json" => Some("div"),
        "op_drop.spec.json" => Some("drop"),
        "op_eq.spec.json" => Some("eq"),
        "op_field.spec.json" => Some("field"),
        "op_for.spec.json" => Some("for"),
        "op_ge.spec.json" => Some("ge"),
        "op_gt.spec.json" => Some("gt"),
        "op_guarded_arm.spec.json" => Some("guarded_arm"),
        "op_if.spec.json" => Some("if"),
        "op_if_let.spec.json" => Some("if_let"),
        "op_index.spec.json" => Some("index"),
        "op_into_iter.spec.json" => Some("into_iter"),
        "op_ite.spec.json" => Some("ite"),
        "op_le.spec.json" => Some("le"),
        "op_len.spec.json" => Some("len"),
        "op_let.spec.json" => Some("let"),
        "op_loop.spec.json" => Some("loop"),
        "op_lt.spec.json" => Some("lt"),
        "op_match.spec.json" => Some("match"),
        "op_match_expr.spec.json" => Some("match_expr"),
        "op_member.spec.json" => Some("member"),
        "op_method_call.spec.json" => Some("method_call"),
        "op_move.spec.json" => Some("move"),
        "op_mul.spec.json" => Some("mul"),
        "op_ne.spec.json" => Some("ne"),
        "op_neg.spec.json" => Some("neg"),
        "op_next.spec.json" => Some("next"),
        "op_not.spec.json" => Some("not"),
        "op_or.spec.json" => Some("or"),
        "op_panic.spec.json" => Some("panic"),
        "op_pattern_bind.spec.json" => Some("pattern_bind"),
        "op_pattern_err.spec.json" => Some("pattern_err"),
        "op_pattern_none.spec.json" => Some("pattern_none"),
        "op_pattern_ok.spec.json" => Some("pattern_ok"),
        "op_pattern_some.spec.json" => Some("pattern_some"),
        "op_pattern_wild.spec.json" => Some("pattern_wild"),
        "op_range.spec.json" => Some("range"),
        "op_range_incl.spec.json" => Some("range_incl"),
        "op_rem.spec.json" => Some("rem"),
        "op_return.spec.json" => Some("return"),
        "op_seq.spec.json" => Some("seq"),
        "op_shl.spec.json" => Some("shl"),
        "op_shr.spec.json" => Some("shr"),
        "op_skip.spec.json" => Some("skip"),
        "op_sub.spec.json" => Some("sub"),
        "op_switch.spec.json" => Some("switch"),
        "op_try.spec.json" => Some("try"),
        "op_try_option.spec.json" => Some("try_option"),
        "op_tuple.spec.json" => Some("tuple"),
        "op_while.spec.json" => Some("while"),
        "op_while_let.spec.json" => Some("while_let"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_core_aliases_to_operation_cids() {
        assert_eq!(op_cid("=="), op_cid("eq"));
        assert_eq!(op_cid("="), op_cid("eq"));
        assert_eq!(op_cid("+"), op_cid("add"));
        assert_eq!(op_cid("method:is_some"), op_cid("method_call"));
        assert_eq!(op_cid("call:foo"), op_cid("call_result"));
        assert!(op_cid("seq").unwrap().starts_with("blake3-512:"));
    }
}
