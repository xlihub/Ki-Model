#[path = "support/gateway.rs"]
mod gateway;

use aion_config::compat::ProviderCompat;
use aion_providers::LlmProvider;
use aion_providers::openai::OpenAIProvider;
use gateway::{StreamingServer, request};

#[tokio::test]
async fn dropping_receiver_closes_an_idle_connection() {
    let mut server = StreamingServer::start().await;
    let provider = OpenAIProvider::new("synthetic-key", &server.url, ProviderCompat::openai_defaults());
    let events = provider.stream(&request()).await.unwrap();
    drop(events);
    server.assert_disconnected().await;
}

#[tokio::test]
async fn text_arrives_before_response_completion_at_byte_boundaries() {
    use aion_types::llm::LlmEvent;
    use tokio::time::{Duration, timeout};
    for ending in ["\n", "\r\n", "\r"] {
        let mut server = StreamingServer::start().await;
        let provider = OpenAIProvider::new("synthetic-key", &server.url, ProviderCompat::openai_defaults());
        let mut events = provider.stream(&request()).await.unwrap();
        let body = format!(
            "\u{feff}:heartbeat{ending}{ending}event:chunk{ending}data:{{{ending}data: \"choices\":[{{\"delta\":{{\"content\":\"你好🌍\"}}}}]}}{ending}{ending}"
        );
        for byte in body.as_bytes() {
            server.send(vec![*byte]).await;
        }
        assert!(
            matches!(timeout(Duration::from_secs(2), events.recv()).await.unwrap(), Some(LlmEvent::TextDelta(text)) if text == "你好🌍")
        );
        assert!(timeout(Duration::from_millis(50), events.recv()).await.is_err());
        server.send(format!("data:{{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}{ending}{ending}data:[DONE]{ending}{ending}")).await;
        assert!(matches!(
            timeout(Duration::from_secs(2), events.recv()).await.unwrap(),
            Some(LlmEvent::Done { .. })
        ));
        assert!(events.recv().await.is_none());
        server.assert_disconnected().await;
    }
}

#[tokio::test]
async fn injected_read_timeout_fails_partial_without_replay() {
    use aion_providers::openai::OpenAIOptions;
    use aion_types::llm::LlmEvent;
    use reqwest::Client;
    use tokio::time::{Duration, timeout};
    let mut server = StreamingServer::start().await;
    let client = Client::builder()
        .no_proxy()
        .read_timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let provider = OpenAIProvider::with_options(
        Some("synthetic-key"),
        &server.url,
        ProviderCompat::openai_defaults(),
        OpenAIOptions {
            client: Some(client),
            ..Default::default()
        },
    )
    .unwrap();
    let mut events = provider.stream(&request()).await.unwrap();
    server
        .send(b"data:{\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n".to_vec())
        .await;
    assert!(matches!(events.recv().await, Some(LlmEvent::TextDelta(_))));
    assert!(matches!(
        timeout(Duration::from_secs(2), events.recv()).await.unwrap(),
        Some(LlmEvent::Error(_))
    ));
    assert!(events.recv().await.is_none());
    server.assert_disconnected().await;
}

#[tokio::test]
async fn injected_connect_timeout_bounds_stalled_tls_handshakes_and_retries() {
    use aion_providers::ProviderError;
    use aion_providers::openai::OpenAIOptions;
    use reqwest::Client;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::net::TcpListener;
    use tokio::time::{Duration, timeout};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("https://{}", listener.local_addr().unwrap());
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let server = tokio::spawn(async move {
        let mut sockets = Vec::new();
        loop {
            sockets.push(listener.accept().await.unwrap().0);
            observed.fetch_add(1, Ordering::SeqCst);
        }
    });
    let client = Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let provider = OpenAIProvider::with_options(
        Some("synthetic-key"),
        &url,
        ProviderCompat::openai_defaults(),
        OpenAIOptions {
            client: Some(client),
            ..Default::default()
        },
    )
    .unwrap();
    let result = timeout(Duration::from_secs(4), provider.stream(&request())).await;
    server.abort();
    assert!(matches!(result.unwrap(), Err(ProviderError::Http(error)) if error.is_timeout()));
    assert_eq!(count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn injected_proxy_and_no_proxy_policies_reach_the_selected_server() {
    use aion_providers::openai::OpenAIOptions;
    use reqwest::{Client, Proxy};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let proxy = MockServer::start().await;
    let direct = MockServer::start().await;
    for server in [&proxy, &direct] {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "data:{\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata:[DONE]\n\n",
                "text/event-stream",
            ))
            .mount(server)
            .await;
    }
    let client = Client::builder()
        .proxy(Proxy::all(proxy.uri()).unwrap())
        .build()
        .unwrap();
    let provider = OpenAIProvider::with_options(
        Some("synthetic-key"),
        "http://model.synthetic.invalid/v1",
        ProviderCompat::openai_defaults(),
        OpenAIOptions {
            client: Some(client),
            ..Default::default()
        },
    )
    .unwrap();
    let mut events = provider.stream(&request()).await.unwrap();
    while events.recv().await.is_some() {}
    let requests = proxy.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].headers["host"], "model.synthetic.invalid");
    assert_eq!(requests[0].url.path(), "/v1/chat/completions");
    let client = Client::builder()
        .proxy(Proxy::all(proxy.uri()).unwrap())
        .no_proxy()
        .build()
        .unwrap();
    let provider = OpenAIProvider::with_options(
        Some("synthetic-key"),
        &direct.uri(),
        ProviderCompat::openai_defaults(),
        OpenAIOptions {
            client: Some(client),
            ..Default::default()
        },
    )
    .unwrap();
    let mut events = provider.stream(&request()).await.unwrap();
    while events.recv().await.is_some() {}
    assert_eq!(proxy.received_requests().await.unwrap().len(), 1);
    assert_eq!(direct.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn injected_total_timeout_also_bounds_waiting_for_response_headers() {
    use aion_providers::ProviderError;
    use aion_providers::openai::OpenAIOptions;
    use reqwest::Client;
    use tokio::time::{Duration, timeout};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let provider = OpenAIProvider::with_options(
        Some("synthetic-key"),
        &server.uri(),
        ProviderCompat::openai_defaults(),
        OpenAIOptions {
            client: Some(client),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        matches!(timeout(Duration::from_secs(1), provider.stream(&request())).await.unwrap(), Err(ProviderError::Http(error)) if error.is_timeout())
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn cancelling_before_response_headers_closes_the_connection() {
    use tokio::time::{Duration, timeout};
    let mut server = StreamingServer::start_with_headers(false).await;
    let provider = OpenAIProvider::new("synthetic-key", &server.url, ProviderCompat::openai_defaults());
    let request = request();
    let mut pending = Box::pin(provider.stream(&request));
    assert!(timeout(Duration::from_millis(100), &mut pending).await.is_err());
    drop(pending);
    server.assert_disconnected().await;
}

#[tokio::test]
async fn cancelling_during_backoff_prevents_another_request() {
    use tokio::time::{Duration, sleep};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(":empty\n\n", "text/event-stream"))
        .mount(&server)
        .await;
    let provider = OpenAIProvider::new("synthetic-key", &server.uri(), ProviderCompat::openai_defaults());
    let events = provider.stream(&request()).await.unwrap();
    sleep(Duration::from_millis(100)).await;
    drop(events);
    sleep(Duration::from_millis(1100)).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_utf8_in_a_comment_does_not_delay_valid_text() {
    use aion_types::llm::LlmEvent;
    use tokio::time::{Duration, timeout};
    let mut server = StreamingServer::start().await;
    let provider = OpenAIProvider::new("synthetic-key", &server.url, ProviderCompat::openai_defaults());
    let mut events = provider.stream(&request()).await.unwrap();
    server
        .send(b": \xff\n\ndata:{\"choices\":[{\"delta\":{\"content\":\"valid text\"}}]}\n\n".to_vec())
        .await;
    assert!(
        matches!(timeout(Duration::from_secs(2), events.recv()).await.unwrap(), Some(LlmEvent::TextDelta(text)) if text == "valid text")
    );
    drop(events);
    server.assert_disconnected().await;
}
