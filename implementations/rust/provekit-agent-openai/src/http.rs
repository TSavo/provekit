// SPDX-License-Identifier: Apache-2.0
//
// Blocking OpenAI-compatible chat completions transport. This module is
// compiled only behind the `live` feature so default CI stays network-free.

use std::env;
use std::io::Write;
use std::process::{Command, Stdio};

use provekit_agent::AgentError;
use serde_json::{json, Value};

use crate::Transport;

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug, Clone)]
pub struct HttpTransport {
    api_key: String,
    base_url: String,
    curl_binary: String,
}

impl HttpTransport {
    pub fn new(api_key: impl Into<String>) -> Result<Self, AgentError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(AgentError::Backend("OPENAI_API_KEY is empty".into()));
        }

        Ok(Self {
            api_key,
            base_url: DEFAULT_BASE_URL.into(),
            curl_binary: "curl".into(),
        })
    }

    pub fn from_env(api_key_env: impl AsRef<str>) -> Result<Self, AgentError> {
        let api_key_env = api_key_env.as_ref();
        let api_key = env::var(api_key_env)
            .map_err(|_| AgentError::Backend(format!("{api_key_env} is not set")))?;
        let base_url = env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
        Ok(Self::new(api_key)?.with_base_url(base_url))
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_curl_binary(mut self, curl_binary: impl Into<String>) -> Self {
        self.curl_binary = curl_binary.into();
        self
    }
}

impl Transport for HttpTransport {
    fn complete(&self, model: &str, system: &str, user: &str) -> Result<String, AgentError> {
        let request_body = serde_json::to_vec(&chat_request_body(model, system, user))
            .map_err(|e| AgentError::Backend(format!("openai request JSON failed: {e}")))?;
        let mut child = Command::new(&self.curl_binary)
            .arg("--silent")
            .arg("--show-error")
            .arg("--fail-with-body")
            .arg("--request")
            .arg("POST")
            .arg("--header")
            .arg(format!("Authorization: Bearer {}", self.api_key))
            .arg("--header")
            .arg("Content-Type: application/json")
            .arg("--data-binary")
            .arg("@-")
            .arg(chat_completions_url(&self.base_url))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AgentError::Io(format!("spawn {}: {e}", self.curl_binary)))?;

        child
            .stdin
            .as_mut()
            .ok_or_else(|| AgentError::Io("curl stdin missing".into()))?
            .write_all(&request_body)
            .map_err(|e| AgentError::Io(format!("write curl stdin: {e}")))?;

        let output = child
            .wait_with_output()
            .map_err(|e| AgentError::Io(format!("wait curl: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(AgentError::Backend(format!(
                "openai HTTP request failed: {stderr}{stdout}"
            )));
        }

        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| AgentError::Backend(format!("openai response JSON failed: {e}")))?;
        chat_response_content(&value)
    }
}

pub fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

pub fn chat_request_body(model: &str, system: &str, user: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": system
            },
            {
                "role": "user",
                "content": user
            }
        ]
    })
}

pub fn chat_response_content(value: &Value) -> Result<String, AgentError> {
    value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .filter(|content| !content.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            AgentError::Backend("openai response missing choices[0].message.content".into())
        })
}
