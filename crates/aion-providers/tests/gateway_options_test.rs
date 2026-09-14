#[path = "support/gateway.rs"]
mod gateway;

use aion_config::compat::ProviderCompat;
use aion_providers::LlmProvider;
use aion_providers::openai::{OpenAIAuth, OpenAIConfigError, OpenAIOptions, OpenAIProvider};
use gateway::request;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SUCCESS: &str =
    "data:{\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata:[DONE]\n\n";

#[tokio::test]
async fn authentication_modes_and_default_options_match_actual_requests() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SUCCESS, "text/event-stream"))
        .mount(&server)
        .await;
    let configurations = [
        (
            OpenAIAuth::Bearer,
            Some("synthetic-key"),
            vec![],
            Some("Bearer synthetic-key"),
        ),
        (
            OpenAIAuth::Bearer,
            Some("synthetic-key"),
            vec![("X-Extra".into(), "synthetic-header".into())],
            Some("Bearer synthetic-key"),
        ),
        (OpenAIAuth::None, None, vec![], None),
        (
            OpenAIAuth::None,
            None,
            vec![("AUTHORIZATION".into(), "Custom synthetic".into())],
            Some("Custom synthetic"),
        ),
        (
            OpenAIAuth::None,
            Some("ignored-key"),
            vec![("X-Extra".into(), "synthetic-header".into())],
            None,
        ),
    ];
    for (auth, key, headers, expected) in configurations {
        let provider = OpenAIProvider::with_options(
            key,
            &server.uri(),
            ProviderCompat::openai_defaults(),
            OpenAIOptions {
                auth,
                headers,
                client: None,
            },
        )
        .unwrap();
        let mut events = provider.stream(&request()).await.unwrap();
        while events.recv().await.is_some() {}
        let requests = server.received_requests().await.unwrap();
        let received = requests.last().unwrap();
        assert_eq!(
            received.headers.get("authorization").map(|h| h.to_str().unwrap()),
            expected
        );
        assert_eq!(received.url.path(), "/chat/completions");
        let body: Value = received.body_json().unwrap();
        assert_eq!(body["stream_options"]["include_usage"], true);
    }
    let legacy = OpenAIProvider::new("synthetic-key", &server.uri(), ProviderCompat::openai_defaults());
    let mut events = legacy.stream(&request()).await.unwrap();
    while events.recv().await.is_some() {}
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].body, requests.last().unwrap().body);
    assert_eq!(requests[0].headers, requests.last().unwrap().headers);
}

#[test]
fn header_validation_is_case_insensitive_and_does_not_disclose_inputs() {
    let cases = [
        (
            vec![("X-Key", "first"), ("x-KEY", "secret-second")],
            OpenAIAuth::None,
            Some("unused"),
            OpenAIConfigError::DuplicateHeader { index: 1 },
        ),
        (
            vec![("Authorization", "secret-conflict")],
            OpenAIAuth::Bearer,
            Some("secret-key"),
            OpenAIConfigError::AuthorizationConflict,
        ),
        (
            vec![("bad\nsecret-name", "secret-value")],
            OpenAIAuth::None,
            None,
            OpenAIConfigError::InvalidHeaderName { index: 0 },
        ),
        (
            vec![("X-Key", "secret\r\nvalue")],
            OpenAIAuth::None,
            None,
            OpenAIConfigError::InvalidHeaderValue { index: 0 },
        ),
        (vec![], OpenAIAuth::Bearer, None, OpenAIConfigError::MissingApiKey),
        (vec![], OpenAIAuth::Bearer, Some("  "), OpenAIConfigError::MissingApiKey),
        (
            vec![],
            OpenAIAuth::Bearer,
            Some("secret\nkey"),
            OpenAIConfigError::InvalidApiKey,
        ),
        (
            vec![("Content-Type", "secret-value")],
            OpenAIAuth::None,
            None,
            OpenAIConfigError::ReservedHeader { index: 0 },
        ),
    ];
    for (headers, auth, key, expected) in cases {
        let options = OpenAIOptions {
            auth,
            headers: headers.into_iter().map(|(k, v)| (k.into(), v.into())).collect(),
            client: None,
        };
        assert!(!format!("{options:?}").contains("secret"));
        let error = OpenAIProvider::with_options(key, "http://synthetic.invalid", ProviderCompat::default(), options)
            .err()
            .unwrap();
        assert_eq!(error, expected);
        assert!(!format!("{error:?} {error}").contains("secret"));
    }
    let mut headers = HeaderMap::new();
    headers.insert("x-client-default", HeaderValue::from_static("secret-client-header"));
    let options = OpenAIOptions {
        client: Some(Client::builder().default_headers(headers).build().unwrap()),
        ..Default::default()
    };
    assert!(!format!("{options:?}").contains("secret-client-header"));
}

#[tokio::test]
async fn api_path_is_inserted_before_base_query_and_full_url_is_unchanged() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SUCCESS, "text/event-stream"))
        .mount(&server)
        .await;
    for (endpoint, api_path, expected_path, expected_query) in [
        (
            "/v1?tenant=synthetic",
            "/chat/completions",
            "/v1/chat/completions",
            "tenant=synthetic",
        ),
        ("/full/?tenant=synthetic/", "", "/full/", "tenant=synthetic/"),
    ] {
        let mut compat = ProviderCompat::openai_defaults();
        compat.transport.api_path = Some(api_path.into());
        let provider = OpenAIProvider::new("synthetic-key", &format!("{}{endpoint}", server.uri()), compat);
        let mut events = provider.stream(&request()).await.unwrap();
        while events.recv().await.is_some() {}
        let requests = server.received_requests().await.unwrap();
        let last = requests.last().unwrap();
        assert_eq!(last.url.path(), expected_path);
        assert_eq!(last.url.query(), Some(expected_query));
    }
}

#[tokio::test]
async fn reflected_credentials_are_redacted_from_http_and_stream_errors() {
    use aion_types::llm::LlmEvent;
    let server = MockServer::start().await;
    let options = OpenAIOptions {
        headers: vec![("X-Key".into(), "synthetic-header-secret".into())],
        ..Default::default()
    };
    let provider = OpenAIProvider::with_options(
        Some("synthetic-bearer-secret"),
        &server.uri(),
        ProviderCompat::openai_defaults(),
        options,
    )
    .unwrap();
    let message = "rejected synthetic-header-secret and synthetic-bearer-secret";
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string(message))
        .mount(&server)
        .await;
    let error = provider.stream(&request()).await.err().unwrap();
    assert!(!format!("{error:?} {error}").contains("synthetic-header-secret"));
    assert!(!format!("{error:?} {error}").contains("synthetic-bearer-secret"));
    server.reset().await;
    let body = format!("data:{{\"error\":{{\"message\":\"{message}\"}}}}\n\n");
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(&server)
        .await;
    let mut events = provider.stream(&request()).await.unwrap();
    let Some(LlmEvent::Error(message)) = events.recv().await else {
        panic!("expected error event")
    };
    assert!(!message.contains("synthetic-header-secret"));
    assert!(!message.contains("synthetic-bearer-secret"));
}
