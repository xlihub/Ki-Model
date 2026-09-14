#[path = "support/gateway.rs"]
mod gateway;

use aion_config::compat::ProviderCompat;
use aion_providers::LlmProvider;
use aion_providers::openai::{OpenAIAuth, OpenAIOptions, OpenAIProvider};
use aion_types::llm::LlmEvent;
use aion_types::message::{ContentBlock, ImageInputCapability, ImageUrl, Message, Role, StopReason};
use aion_types::tool::ToolDef;
use gateway::request;
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn image_reasoning_tool_metadata_and_usage_survive_a_complete_tool_roundtrip() {
    let server = MockServer::start().await;
    let frames = [
        json!({"choices":[{"delta":{"reasoning_content":"checking"}}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_synthetic","function":{"name":"add","arguments":"{\"value\":"},"extra_content":{"synthetic":{"signature":"opaque-metadata"}}}]}}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"2}"}}]},"finish_reason":"tool_calls"}]}),
        json!({"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":3}}}),
    ];
    let body = frames
        .iter()
        .map(|frame| format!("data:{frame}\n\n"))
        .collect::<String>()
        + "data:[DONE]\n\n";
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(&server)
        .await;
    let mut compat = ProviderCompat::openai_defaults();
    compat.image_input = Some(ImageInputCapability::Supported);
    let provider = OpenAIProvider::with_options(
        None,
        &server.uri(),
        compat,
        OpenAIOptions {
            auth: OpenAIAuth::None,
            headers: vec![("X-Synthetic-Key".into(), "synthetic-secret".into())],
            ..Default::default()
        },
    )
    .unwrap();
    let mut request = request();
    let image = "data:image/png;base64,AA==";
    request.messages[0].content.push(ContentBlock::Image {
        image_url: ImageUrl { url: image.into() },
    });
    request.tools.push(ToolDef {
        name: "add".into(),
        description: "Add one".into(),
        input_schema: json!({"type":"object","properties":{"value":{"type":"integer"}},"required":["value"]}),
        deferred: false,
    });
    let mut events = provider.stream(&request).await.unwrap();
    assert!(matches!(events.recv().await, Some(LlmEvent::ThinkingDelta(text)) if text == "checking"));
    let Some(LlmEvent::ToolUse { id, name, input, extra }) = events.recv().await else {
        panic!("missing tool event")
    };
    assert_eq!(id, "call_synthetic");
    assert_eq!(name, "add");
    assert_eq!(input, json!({"value":2}));
    assert_eq!(extra, Some(json!({"synthetic":{"signature":"opaque-metadata"}})));
    assert!(
        matches!(events.recv().await, Some(LlmEvent::Done { stop_reason: StopReason::ToolUse, usage }) if usage.input_tokens == 12 && usage.output_tokens == 5 && usage.cache_read_tokens == 3)
    );
    assert!(events.recv().await.is_none());
    let first = server.received_requests().await.unwrap();
    let body: Value = first[0].body_json().unwrap();
    assert_eq!(body["messages"][0]["content"][0]["text"], "Hello");
    assert_eq!(body["messages"][0]["content"][1]["image_url"]["url"], image);
    assert_eq!(body["tools"][0]["function"]["name"], "add");
    assert_eq!(body["tools"][0]["function"]["parameters"]["required"], json!(["value"]));
    assert!(first[0].headers.get("authorization").is_none());
    let result = input["value"].as_i64().unwrap() + 1;
    request.messages.push(Message::new(
        Role::Assistant,
        vec![
            ContentBlock::Thinking {
                thinking: "checking".into(),
                signature: None,
            },
            ContentBlock::ToolUse {
                id: id.clone(),
                name,
                input,
                extra,
            },
        ],
    ));
    request.messages.push(Message::new(
        Role::Tool,
        vec![ContentBlock::ToolResult {
            tool_use_id: id,
            content: result.to_string(),
            is_error: false,
        }],
    ));
    server.reset().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_raw("data:{\"choices\":[{\"delta\":{\"content\":\"The result is 3\"},\"finish_reason\":\"stop\"}]}\n\ndata:[DONE]\n\n", "text/event-stream")).mount(&server).await;
    let mut events = provider.stream(&request).await.unwrap();
    assert!(matches!(events.recv().await, Some(LlmEvent::TextDelta(text)) if text == "The result is 3"));
    assert!(matches!(
        events.recv().await,
        Some(LlmEvent::Done {
            stop_reason: StopReason::EndTurn,
            ..
        })
    ));
    assert!(events.recv().await.is_none());
    let second = server.received_requests().await.unwrap();
    assert_eq!(second.len(), 1);
    let body: Value = second[0].body_json().unwrap();
    assert_eq!(body["messages"][1]["reasoning_content"], "checking");
    assert_eq!(
        body["messages"][1]["tool_calls"][0]["extra_content"],
        json!({"synthetic":{"signature":"opaque-metadata"}})
    );
    assert_eq!(
        body["messages"][2],
        json!({"role":"tool","tool_call_id":"call_synthetic","content":"3"})
    );
    assert_eq!(second[0].headers["x-synthetic-key"], "synthetic-secret");
}

#[tokio::test]
async fn failure_after_tool_delivery_never_replays_the_tool() {
    let server = MockServer::start().await;
    let body = concat!(
        "data:{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_synthetic\",\"function\":{\"name\":\"add\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data:{\"error\":{\"code\":429,\"message\":\"synthetic rate limit\"}}\n\n"
    );
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(&server)
        .await;
    let provider = OpenAIProvider::new("synthetic-key", &server.uri(), ProviderCompat::openai_defaults());
    let mut events = provider.stream(&request()).await.unwrap();
    assert!(matches!(events.recv().await, Some(LlmEvent::ToolUse { .. })));
    assert!(matches!(events.recv().await, Some(LlmEvent::Error(_))));
    assert!(events.recv().await.is_none());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
