#![cfg(feature = "live")]

use provekit_agent_openai::http::{
    chat_completions_url, chat_request_body, chat_response_content, HttpTransport,
};
use serde_json::json;

#[test]
fn chat_completions_url_appends_the_endpoint_once() {
    assert_eq!(
        chat_completions_url("https://api.example.test/v1/"),
        "https://api.example.test/v1/chat/completions"
    );
    assert_eq!(
        chat_completions_url("https://api.example.test/v1/chat/completions"),
        "https://api.example.test/v1/chat/completions"
    );
}

#[test]
fn request_body_carries_model_and_messages() {
    let body = chat_request_body("gpt-test", "system prompt", "user prompt");

    assert_eq!(body["model"], "gpt-test");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "system prompt");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "user prompt");
}

#[test]
fn response_content_extracts_first_choice_text() {
    let response = json!({
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": "{\"name\":\"ok\"}"
                }
            }
        ]
    });

    assert_eq!(
        chat_response_content(&response).expect("content"),
        "{\"name\":\"ok\"}"
    );
}

#[test]
fn http_transport_rejects_empty_api_key() {
    assert!(HttpTransport::new(" ").is_err());
}
