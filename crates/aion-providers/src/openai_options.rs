use std::fmt;

use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use thiserror::Error;

/// Authentication applied by the SDK. Bearer credentials come only from the
/// `api_key` argument to `OpenAIProvider::with_options`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OpenAIAuth {
    #[default]
    Bearer,
    /// Do not add Bearer authentication. Custom headers may provide auth.
    None,
}

/// Optional request headers and caller-owned HTTP policy for OpenAI providers.
///
/// Defaults preserve the legacy Bearer/client behavior. Keep credentials in
/// `api_key` or `headers`; injected clients should configure network policy,
/// not default authentication headers. Client clones share a connection pool.
#[derive(Clone, Default)]
pub struct OpenAIOptions {
    pub auth: OpenAIAuth,
    /// Header names are case-insensitive. Duplicate and reserved headers fail
    /// construction; values are always treated as sensitive.
    pub headers: Vec<(String, String)>,
    /// Used as-is, including proxy, connect/read/total timeouts and TLS policy.
    pub client: Option<Client>,
}

impl fmt::Debug for OpenAIOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAIOptions")
            .field("auth", &self.auth)
            .field("header_count", &self.headers.len())
            .field("custom_client", &self.client.is_some())
            .finish()
    }
}

/// Configuration errors never contain supplied header names or values.
#[derive(Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum OpenAIConfigError {
    #[error("Bearer authentication requires a non-empty API key")]
    MissingApiKey,
    #[error("Invalid Bearer credential")]
    InvalidApiKey,
    #[error("Invalid header name at index {index}")]
    InvalidHeaderName { index: usize },
    #[error("Invalid header value at index {index}")]
    InvalidHeaderValue { index: usize },
    #[error("Duplicate header at index {index} (names are case-insensitive)")]
    DuplicateHeader { index: usize },
    #[error("Authorization header conflicts with Bearer authentication")]
    AuthorizationConflict,
    #[error("Header at index {index} is controlled by the HTTP transport")]
    ReservedHeader { index: usize },
}

impl OpenAIOptions {
    pub(crate) fn request_headers(&self, api_key: Option<&str>) -> Result<HeaderMap, OpenAIConfigError> {
        let mut headers = HeaderMap::new();
        for (index, (name, value)) in self.headers.iter().enumerate() {
            let name =
                HeaderName::from_bytes(name.as_bytes()).map_err(|_| OpenAIConfigError::InvalidHeaderName { index })?;
            if headers.contains_key(&name) {
                return Err(OpenAIConfigError::DuplicateHeader { index });
            }
            if name == AUTHORIZATION && self.auth == OpenAIAuth::Bearer {
                return Err(OpenAIConfigError::AuthorizationConflict);
            }
            if matches!(
                name.as_str(),
                "content-type" | "content-length" | "transfer-encoding" | "host"
            ) {
                return Err(OpenAIConfigError::ReservedHeader { index });
            }
            let mut value =
                HeaderValue::from_str(value).map_err(|_| OpenAIConfigError::InvalidHeaderValue { index })?;
            value.set_sensitive(true);
            headers.insert(name, value);
        }
        if self.auth == OpenAIAuth::Bearer {
            let key = api_key
                .filter(|key| !key.trim().is_empty())
                .ok_or(OpenAIConfigError::MissingApiKey)?;
            let mut value =
                HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| OpenAIConfigError::InvalidApiKey)?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }
}
