#![allow(dead_code)]
use aion_types::llm::LlmRequest;
use aion_types::message::{ContentBlock, Message, Role};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

pub(super) fn request() -> LlmRequest {
    LlmRequest {
        model: "synthetic-model".into(),
        system: String::new(),
        messages: vec![Message::new(
            Role::User,
            vec![ContentBlock::Text { text: "Hello".into() }],
        )],
        tools: vec![],
        max_tokens: Some(64),
        thinking: None,
        reasoning_effort: None,
    }
}

/// Synthetic HTTP/1.1 server with controllable response chunks and an observable
/// peer disconnect. Dropping the harness always aborts its server task.
pub(super) struct StreamingServer {
    pub(super) url: String,
    chunks: mpsc::Sender<Vec<u8>>,
    disconnected: oneshot::Receiver<()>,
    task: JoinHandle<()>,
}

impl StreamingServer {
    pub(super) async fn start() -> Self {
        Self::start_with_headers(true).await
    }

    pub(super) async fn start_with_headers(send_headers: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (chunks, mut rx) = mpsc::channel::<Vec<u8>>(1024);
        let (closed, disconnected) = oneshot::channel();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    return;
                }
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let len: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            if send_headers {
                socket.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n").await.unwrap();
            }
            loop {
                tokio::select! {
                    result = socket.read(&mut buf) => {
                        assert!(matches!(result, Ok(0) | Err(_)), "unexpected request after stream");
                        let _ = closed.send(());
                        return;
                    }
                    chunk = rx.recv() => {
                        let Some(chunk) = chunk else {
                            let _ = socket.write_all(b"0\r\n\r\n").await;
                            return;
                        };
                        let prefix = format!("{:x}\r\n", chunk.len());
                        if socket.write_all(prefix.as_bytes()).await.is_err()
                            || socket.write_all(&chunk).await.is_err()
                            || socket.write_all(b"\r\n").await.is_err() {
                            let _ = closed.send(());
                            return;
                        }
                    }
                }
            }
        });
        Self {
            url,
            chunks,
            disconnected,
            task,
        }
    }

    pub(super) async fn send(&self, body: impl Into<Vec<u8>>) {
        self.chunks.send(body.into()).await.unwrap();
    }

    pub(super) async fn assert_disconnected(&mut self) {
        timeout(Duration::from_secs(2), &mut self.disconnected)
            .await
            .expect("provider retained connection")
            .unwrap();
    }
}

impl Drop for StreamingServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
