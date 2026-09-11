use async_trait::async_trait;
#[cfg(test)]
use serde_json::Value;
use tokio::sync::mpsc;

use aion_config::compat::ProviderCompat;
use aion_types::llm::{LlmEvent, LlmRequest};

use crate::error::ProviderError;
use crate::provider::LlmProvider;
use crate::stream_runner::run_stream;
use crate::transport::ProviderTransport;

#[derive(Clone)]
pub(crate) struct ComposedProvider {
    transport: ProviderTransport,
    compat: ProviderCompat,
}

impl ComposedProvider {
    pub(crate) fn new(transport: ProviderTransport, compat: ProviderCompat) -> Self {
        Self { transport, compat }
    }

    #[cfg(test)]
    pub(crate) fn build_request_body(&self, request: &LlmRequest) -> Result<Value, ProviderError> {
        let (body, _) = self.transport.project_body(request, &self.compat)?;
        Ok(body)
    }
}

#[async_trait]
impl LlmProvider for ComposedProvider {
    async fn stream(&self, request: &LlmRequest) -> Result<mpsc::Receiver<LlmEvent>, ProviderError> {
        let (body, tool_wire_shape) = self.transport.project_body(request, &self.compat)?;

        tracing::debug!(
            target: "aion_providers",
            model = %request.model,
            message_count = request.messages.len(),
            tool_count = request.tools.len(),
            max_tokens = ?request.max_tokens,
            thinking_configured = request.thinking.is_some(),
            "provider request projected"
        );

        let transport = self.transport.clone();
        let compat = self.compat.clone();
        let model = request.model.clone();
        let send = move || {
            let transport = transport.clone();
            let compat = compat.clone();
            let body = body.clone();
            let model = model.clone();
            async move {
                let redactor = transport.error_redactor();
                let projected_request = transport
                    .build_projected_request(&model, body, &compat, tool_wire_shape)
                    .map_err(|error| redactor.error(error))?;
                transport
                    .send(projected_request)
                    .await
                    .map_err(|error| redactor.error(error))
            }
        };

        let decoder = self.transport.decoder(&self.compat);
        let redactor = self.transport.error_redactor();
        let process = move |response, tx| {
            let redactor = redactor.clone();
            async move { decoder.process(response, &tx, &redactor).await }
        };
        let retry_policy = self.transport.retry_policy();

        run_stream(send, process, retry_policy).await
    }
}

#[cfg(test)]
#[path = "composed_test.rs"]
mod composed_test;
