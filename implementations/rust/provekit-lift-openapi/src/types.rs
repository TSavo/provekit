use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Diagnostics {
    pub messages: Vec<String>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    pub fn push(&mut self, msg: impl Into<String>) {
        self.messages.push(msg.into());
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}

pub enum Declaration {
    Contract(ContractDecl),
    Bridge(BridgeDecl),
}

#[derive(Debug, Clone, Serialize)]
pub struct ContractDecl {
    pub name: String,
    #[serde(rename = "outBinding")]
    pub out_binding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inv: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeDecl {
    pub name: String,
    #[serde(rename = "sourceSymbol")]
    pub source_symbol: String,
    #[serde(rename = "sourceLayer")]
    pub source_layer: String,
    #[serde(rename = "sourceContractCid")]
    pub source_contract_cid: String,
    #[serde(rename = "targetContractCid")]
    pub target_contract_cid: String,
    #[serde(rename = "targetProofCid")]
    pub target_proof_cid: String,
    #[serde(rename = "targetLayer")]
    pub target_layer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Declaration {
    pub fn to_json(&self) -> Value {
        match self {
            Declaration::Contract(c) => {
                let mut v = serde_json::to_value(c).unwrap_or_default();
                if let Value::Object(map) = &mut v {
                    map.insert("kind".to_string(), Value::String("contract".to_string()));
                }
                v
            }
            Declaration::Bridge(b) => {
                let mut v = serde_json::to_value(b).unwrap_or_default();
                if let Value::Object(map) = &mut v {
                    map.insert("kind".to_string(), Value::String("bridge".to_string()));
                }
                v
            }
        }
    }
}
