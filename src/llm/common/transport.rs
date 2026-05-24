use std::time::Duration;

use super::{LlmConfig, LlmError};

const MAX_LLM_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

/// HTTP transport boundary. Production uses [`UreqTransport`]; tests inject
/// a mock so the extractor can be exercised without a network.
pub trait LlmTransport: Send + Sync {
    /// POST `body` (JSON) to `url` with the supplied headers, returning
    /// `(status, body)`. Implementations MUST NOT raise on non-2xx - let
    /// the caller decide based on status.
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<(u16, String), LlmError>;
}

/// `ureq`-backed transport. Synchronous to fit the rest of aghist's IO model.
pub struct UreqTransport {
    timeout: Duration,
}

impl UreqTransport {
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new(Duration::from_secs(LlmConfig::DEFAULT_TIMEOUT_SECS))
    }
}

impl LlmTransport for UreqTransport {
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<(u16, String), LlmError> {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .http_status_as_error(false)
            .build();
        let agent = ureq::Agent::from(config);
        let mut req = agent.post(url).header("content-type", "application/json");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        match req.send(body) {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                let text = resp
                    .body_mut()
                    .with_config()
                    .limit(MAX_LLM_RESPONSE_BYTES)
                    .read_to_string()
                    .map_err(|e| LlmError::Http {
                        url: url.to_string(),
                        source: Box::new(e),
                    })?;
                Ok((status, text))
            }
            Err(e) => Err(LlmError::Http {
                url: url.to_string(),
                source: Box::new(e),
            }),
        }
    }
}

fn request_headers(config: &LlmConfig) -> [(&str, &str); 3] {
    [
        ("x-api-key", config.api_key.as_str()),
        ("anthropic-version", config.anthropic_version.as_str()),
        ("content-type", "application/json"),
    ]
}

pub(crate) fn post_request<T: LlmTransport + ?Sized>(
    transport: &T,
    config: &LlmConfig,
    body: &str,
) -> Result<String, LlmError> {
    let headers = request_headers(config);
    let (status, resp_body) = transport.post_json(&config.endpoint, &headers, body)?;
    if !(200..300).contains(&status) {
        return Err(LlmError::ApiStatus {
            url: config.endpoint.clone(),
            status,
            body: resp_body.chars().take(500).collect(),
        });
    }
    Ok(resp_body)
}
