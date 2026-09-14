use std::cmp::Reverse;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap};

use crate::ProviderError;

/// Known request credentials only; never inspect prompts or response content.
#[derive(Clone, Default)]
pub(crate) struct ErrorRedactor {
    secrets: Vec<String>,
}

impl ErrorRedactor {
    pub(crate) fn new(headers: Option<&HeaderMap>, legacy_key: &str) -> Self {
        let mut secrets = Vec::new();
        if !legacy_key.is_empty() {
            secrets.push(legacy_key.to_string());
        }
        if let Some(headers) = headers {
            for (name, value) in headers {
                if name == CONTENT_TYPE {
                    continue;
                }
                let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
                if name == AUTHORIZATION
                    && let Some(key) = value.strip_prefix("Bearer ")
                    && !key.is_empty()
                {
                    secrets.push(key.to_string());
                }
                if !value.is_empty() {
                    secrets.push(value);
                }
            }
        }
        secrets.sort_by_key(|secret| Reverse(secret.len()));
        secrets.dedup();
        Self { secrets }
    }

    pub(crate) fn text(&self, mut text: String) -> String {
        for secret in &self.secrets {
            text = text.replace(secret, "[REDACTED]");
        }
        text
    }

    pub(crate) fn error(&self, error: ProviderError) -> ProviderError {
        match error {
            ProviderError::Http(error) => ProviderError::Http(error.without_url()),
            ProviderError::Api { status, message } => ProviderError::Api {
                status,
                message: self.text(message),
            },
            ProviderError::Parse(message) => ProviderError::Parse(self.text(message)),
            ProviderError::Connection(message) => ProviderError::Connection(self.text(message)),
            ProviderError::PromptTooLong(message) => ProviderError::PromptTooLong(self.text(message)),
            ProviderError::RateLimited { retry_after_ms, body } => ProviderError::RateLimited {
                retry_after_ms,
                body: body.map(|body| self.text(body)),
            },
        }
    }
}
