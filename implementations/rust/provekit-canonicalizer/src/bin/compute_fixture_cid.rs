use provekit_canonicalizer::{encode_jcs, blake3_512_of, Value as CValue};
use std::fs;
use std::sync::Arc;

fn to_cvalue(v: &serde_json::Value) -> Arc<CValue> {
    match v {
        serde_json::Value::Null => CValue::null(),
        serde_json::Value::Bool(b) => CValue::boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else if let Some(f) = n.as_f64() {
                CValue::string(format!("{}", f))
            } else {
                CValue::null()
            }
        }
        serde_json::Value::String(s) => CValue::string(s.clone()),
        serde_json::Value::Array(arr) => {
            CValue::array(arr.iter().map(|v| to_cvalue(v)).collect())
        }
        serde_json::Value::Object(obj) => {
            CValue::object(obj.iter().map(|(k, v)| (k.clone(), to_cvalue(v))))
        }
    }
}

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest_dir)
        .parent().unwrap()
        .parent().unwrap()
        .parent().unwrap();
    let fixture = repo_root.join("protocol/conformance/2026-05-05-sort-region-and-dependent-byte-pinned.json");
    let json_str = fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read fixture {:?}: {}", fixture, e));
    let v: serde_json::Value = serde_json::from_str(&json_str)
        .unwrap_or_else(|e| panic!("parse JSON: {}", e));
    let cv = to_cvalue(&v);
    let jcs = encode_jcs(&cv);
    let cid = blake3_512_of(jcs.as_bytes());
    println!("{}", cid);
}
