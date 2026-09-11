#[path = "support/gateway.rs"]
mod gateway;

use aion_config::compat::ProviderCompat;
use aion_providers::{LlmProvider, openai::OpenAIProvider};
use aion_types::llm::LlmEvent;
use aion_types::message::StopReason;
use gateway::request;
use tokio::time::{Duration, timeout};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn standard_sse_multiline_data_works_with_legacy_constructor() {
    let server = MockServer::start().await;
    let body = concat!(
        ": comment\r\n\r\n",
        "data:{\r\n",
        "data: \"choices\":[{\"delta\":{\"content\":\"你好\"},\"finish_reason\":\"stop\"}]}\r\n\r\n",
        "data:[DONE]\r\n\r\n",
    );
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(&server)
        .await;
    let provider = OpenAIProvider::new("synthetic-key", &server.uri(), ProviderCompat::openai_defaults());
    let mut events = provider.stream(&request()).await.unwrap();
    assert!(
        matches!(timeout(Duration::from_secs(2), events.recv()).await.unwrap(), Some(LlmEvent::TextDelta(text)) if text == "你好")
    );
    assert!(matches!(
        events.recv().await,
        Some(LlmEvent::Done {
            stop_reason: StopReason::EndTurn,
            ..
        })
    ));
    assert!(events.recv().await.is_none());
}

#[tokio::test]
async fn header_only_auth_uses_full_url_and_preserves_query() {
    use aion_providers::openai::{OpenAIAuth, OpenAIOptions};
    use serde_json::Value;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            "text/event-stream",
        ))
        .mount(&server)
        .await;
    let mut compat = ProviderCompat::openai_defaults();
    compat.transport.api_path = Some(String::new());
    compat.transport.include_stream_options = Some(false);
    let provider = OpenAIProvider::with_options(
        None,
        &format!("{}/gateway/chat?tenant=synthetic", server.uri()),
        compat,
        OpenAIOptions {
            auth: OpenAIAuth::None,
            headers: vec![("X-Synthetic-Key".into(), "synthetic-secret".into())],
            ..Default::default()
        },
    )
    .unwrap();
    let mut events = provider.stream(&request()).await.unwrap();
    while let Some(event) = events.recv().await {
        assert!(!matches!(event, LlmEvent::Error(_)), "{event:?}");
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/gateway/chat");
    assert_eq!(requests[0].url.query(), Some("tenant=synthetic"));
    assert_eq!(requests[0].headers["x-synthetic-key"], "synthetic-secret");
    assert!(!requests[0].headers.contains_key("authorization"));
    let body: Value = requests[0].body_json().unwrap();
    assert_eq!(body["model"], "synthetic-model");
    assert_eq!(body["stream"], true);
    assert!(body.get("stream_options").is_none());
}

#[tokio::test]
async fn invalid_json_cannot_be_hidden_by_done() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("data: {invalid}\n\ndata: [DONE]\n\n", "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = OpenAIProvider::new("synthetic-key", &server.uri(), ProviderCompat::openai_defaults());
    let mut events = provider.stream(&request()).await.unwrap();
    assert!(matches!(events.recv().await, Some(LlmEvent::Error(_))));
    assert!(events.recv().await.is_none());
    server.verify().await;
}

#[tokio::test]
async fn reasoning_output_is_not_replayed_after_disconnect() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "data:{\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking\"}}]}\n\n",
            "text/event-stream",
        ))
        .mount(&server)
        .await;
    let provider = OpenAIProvider::new("synthetic-key", &server.uri(), ProviderCompat::openai_defaults());
    let mut events = provider.stream(&request()).await.unwrap();
    assert!(matches!(events.recv().await, Some(LlmEvent::ThinkingDelta(text)) if text == "thinking"));
    assert!(matches!(events.recv().await, Some(LlmEvent::Error(_))));
    assert!(events.recv().await.is_none());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn done_without_finish_reason_is_a_bounded_failure() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("data:[DONE]\n\n", "text/event-stream"))
        .mount(&server)
        .await;
    let provider = OpenAIProvider::new("synthetic-key", &server.uri(), ProviderCompat::openai_defaults());
    let mut events = provider.stream(&request()).await.unwrap();
    assert!(matches!(
        timeout(Duration::from_secs(6), events.recv()).await.unwrap(),
        Some(LlmEvent::Error(_))
    ));
    assert!(events.recv().await.is_none());
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn unfinished_event_after_finish_reason_is_not_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "data:{\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"stop\"}]}\n\ndata:{\"usage\":",
            "text/event-stream",
        ))
        .mount(&server)
        .await;
    let provider = OpenAIProvider::new("synthetic-key", &server.uri(), ProviderCompat::openai_defaults());
    let mut events = provider.stream(&request()).await.unwrap();
    assert!(matches!(events.recv().await, Some(LlmEvent::TextDelta(_))));
    assert!(matches!(events.recv().await, Some(LlmEvent::Error(_))));
    assert!(events.recv().await.is_none());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn responses_and_anthropic_use_the_same_standard_framing() {
    use aion_config::compat::OpenAiApiMode;
    use aion_providers::anthropic::AnthropicProvider;
    let server = MockServer::start().await;
    let responses = concat!(
        "event:response.output_text.delta\rdata:{\rdata: \"type\":\"response.output_text.delta\",\"delta\":\"response text\"}\r\r",
        "data:{\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":8,\"output_tokens\":3}}}\r\r"
    );
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(responses, "text/event-stream"))
        .mount(&server)
        .await;
    let mut compat = ProviderCompat::openai_defaults();
    compat.transport.openai_api_mode = Some(OpenAiApiMode::Responses);
    let provider = OpenAIProvider::new("synthetic-key", &server.uri(), compat);
    let mut events = provider.stream(&request()).await.unwrap();
    assert!(matches!(events.recv().await, Some(LlmEvent::TextDelta(text)) if text == "response text"));
    assert!(
        matches!(events.recv().await, Some(LlmEvent::Done { usage, .. }) if usage.input_tokens == 8 && usage.output_tokens == 3)
    );
    assert!(events.recv().await.is_none());
    server.reset().await;
    let anthropic = concat!(
        "event:content_block_delta\r\ndata:{\r\ndata: \"delta\":{\"type\":\"text_delta\",\"text\":\"anthropic text\"}}\r\n\r\n",
        "event:message_delta\r\ndata:{\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\r\n\r\n"
    );
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(anthropic, "text/event-stream"))
        .mount(&server)
        .await;
    let provider = AnthropicProvider::new("synthetic-key", &server.uri(), ProviderCompat::anthropic_defaults());
    let mut events = provider.stream(&request()).await.unwrap();
    assert!(matches!(events.recv().await, Some(LlmEvent::TextDelta(text)) if text == "anthropic text"));
    assert!(
        matches!(events.recv().await, Some(LlmEvent::Done { stop_reason: StopReason::EndTurn, usage }) if usage.output_tokens == 4)
    );
    assert!(events.recv().await.is_none());
}
