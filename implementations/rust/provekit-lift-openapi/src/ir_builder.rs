use serde_json::{json, Value};

fn make_sort(name: &str) -> Value {
    json!({"kind": "primitive", "name": name})
}

pub fn string_sort() -> Value {
    make_sort("String")
}
pub fn int_sort() -> Value {
    make_sort("Int")
}
pub fn bool_sort() -> Value {
    make_sort("Bool")
}
pub fn real_sort() -> Value {
    make_sort("Real")
}
pub fn ref_sort() -> Value {
    make_sort("Ref")
}

pub fn var(name: &str) -> Value {
    json!({ "kind": "var", "name": name })
}

pub fn const_int(value: i64) -> Value {
    json!({ "kind": "const", "value": value, "sort": int_sort() })
}

pub fn const_string(value: &str) -> Value {
    json!({ "kind": "const", "value": value, "sort": string_sort() })
}

pub fn const_bool(value: bool) -> Value {
    json!({ "kind": "const", "value": value, "sort": bool_sort() })
}

pub fn const_real(value: f64) -> Value {
    json!({ "kind": "const", "value": Value::Number(serde_json::Number::from_f64(value).unwrap_or(serde_json::Number::from(0))), "sort": real_sort() })
}

pub fn ctor(name: &str, args: Vec<Value>) -> Value {
    json!({ "kind": "ctor", "name": name, "args": args })
}

pub fn field_access(field_name: &str, target: &str) -> Value {
    ctor(field_name, vec![var(target)])
}

pub fn field_access_val(field_name: &str, target: &Value) -> Value {
    ctor(field_name, vec![target.clone()])
}

pub fn atomic(name: &str, args: Vec<Value>) -> Value {
    json!({ "kind": "atomic", "name": name, "args": args })
}

pub fn eq(a: Value, b: Value) -> Value {
    atomic("=", vec![a, b])
}

pub fn ne(a: Value, b: Value) -> Value {
    atomic("\u{2260}", vec![a, b])
}

pub fn gt(a: Value, b: Value) -> Value {
    atomic(">", vec![a, b])
}

pub fn gte(a: Value, b: Value) -> Value {
    atomic("\u{2265}", vec![a, b])
}

pub fn lt(a: Value, b: Value) -> Value {
    atomic("<", vec![a, b])
}

pub fn lte(a: Value, b: Value) -> Value {
    atomic("\u{2264}", vec![a, b])
}

pub fn and(operands: Vec<Value>) -> Value {
    json!({ "kind": "and", "operands": operands })
}

pub fn or(operands: Vec<Value>) -> Value {
    json!({ "kind": "or", "operands": operands })
}

pub fn not(operand: Value) -> Value {
    json!({ "kind": "not", "operands": [operand] })
}

pub fn implies(antecedent: Value, consequent: Value) -> Value {
    json!({ "kind": "implies", "operands": [antecedent, consequent] })
}

pub fn forall(var_name: &str, sort: Value, body: Value) -> Value {
    json!({
        "kind": "forall",
        "name": var_name,
        "sort": sort,
        "body": body
    })
}

pub fn exists(var_name: &str, sort: Value, body: Value) -> Value {
    json!({
        "kind": "exists",
        "name": var_name,
        "sort": sort,
        "body": body
    })
}

pub fn forall_ref(var_name: &str, body: Value) -> Value {
    forall(var_name, ref_sort(), body)
}

pub fn matches(field: Value, pattern: &str) -> Value {
    atomic("matches", vec![field, const_string(pattern)])
}

pub fn is_null(field: Value) -> Value {
    atomic("is_null", vec![field])
}

pub fn not_null(field: Value) -> Value {
    atomic("not_null", vec![field])
}

pub fn length_of(field: Value) -> Value {
    ctor("length", vec![field])
}

pub fn len_gte(field: Value, n: i64) -> Value {
    gte(length_of(field), const_int(n))
}

pub fn len_lte(field: Value, n: i64) -> Value {
    lte(length_of(field), const_int(n))
}

pub fn len_eq(field: Value, n: i64) -> Value {
    eq(length_of(field), const_int(n))
}

pub fn member_of(value: Value, set_values: Vec<Value>) -> Value {
    let set_ctor = ctor("Set", set_values);
    atomic("member", vec![value, set_ctor])
}

pub fn contract_with_post(name: &str, out_binding: &str, post_formula: Value) -> Value {
    json!({
        "kind": "contract",
        "name": name,
        "outBinding": out_binding,
        "post": post_formula
    })
}

pub fn contract_with_pre(name: &str, out_binding: &str, pre_formula: Value) -> Value {
    json!({
        "kind": "contract",
        "name": name,
        "outBinding": out_binding,
        "pre": pre_formula
    })
}
