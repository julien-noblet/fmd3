use reqwest::{header, Client, Response};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub mod ssrf;
pub use ssrf::{is_safe_ip, validate_safe_url};

#[derive(Error, Debug)]
pub enum NetError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("SSRF Blocked: {0}")]
    SsrfBlocked(String),

    #[error("FlareSolverr error: {0}")]
    FlareSolverr(String),

    #[error("Network error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, NetError>;

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(
                "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0",
            ),
        );
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
            ),
        );
        headers.insert(
            header::ACCEPT_LANGUAGE,
            header::HeaderValue::from_static("en-US,en;q=0.5"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self { client })
    }

    pub async fn get(&self, url: &str) -> Result<Response> {
        let resp = self.client.get(url).send().await?;
        Ok(resp)
    }

    pub async fn get_text(&self, url: &str) -> Result<String> {
        let resp = self.get(url).await?;
        let text = resp.text().await?;
        Ok(text)
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self.get(url).await?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    pub async fn get_text_with_headers(
        &self,
        url: &str,
        custom_headers: &std::collections::HashMap<String, String>,
    ) -> Result<(u16, String)> {
        let mut req = self.client.get(url);
        for (k, v) in custom_headers {
            if let Ok(hn) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                if let Ok(hv) = reqwest::header::HeaderValue::from_str(v) {
                    req = req.header(hn, hv);
                }
            }
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();

        // Fallback to FlareSolverr if Cloudflare block and configured
        if status == 403 || status == 503 {
            if let Ok(fs_url) = std::env::var("FLARESOLVERR_URL") {
                if !fs_url.is_empty() {
                    let fc = FlareSolverrClient::new(fs_url);
                    if let Ok(html) = fc.resolve_html(url).await {
                        return Ok((200, html));
                    }
                }
            }
        }

        let text = resp.text().await?;
        Ok((status, text))
    }

    pub async fn get_bytes_with_headers(
        &self,
        url: &str,
        custom_headers: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<u8>> {
        let (status, bytes) = self
            .get_bytes_and_status_with_headers(url, custom_headers)
            .await?;
        if status >= 400 {
            return Err(NetError::Other(format!("HTTP error {} on {}", status, url)));
        }
        Ok(bytes)
    }

    pub async fn get_bytes_and_status_with_headers(
        &self,
        url: &str,
        custom_headers: &std::collections::HashMap<String, String>,
    ) -> Result<(u16, Vec<u8>)> {
        let mut req = self.client.get(url);
        for (k, v) in custom_headers {
            if let Ok(hn) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                if let Ok(hv) = reqwest::header::HeaderValue::from_str(v) {
                    req = req.header(hn, hv);
                }
            }
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let bytes = resp.bytes().await?;
        Ok((status, bytes.to_vec()))
    }

    pub async fn post_text_with_headers(
        &self,
        url: &str,
        body: &str,
        mime_type: Option<&str>,
        custom_headers: &std::collections::HashMap<String, String>,
    ) -> Result<(u16, String)> {
        let mut req = self.client.post(url);
        let ct = mime_type.unwrap_or("application/x-www-form-urlencoded");
        req = req.header(reqwest::header::CONTENT_TYPE, ct);
        for (k, v) in custom_headers {
            if let Ok(hn) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                if let Ok(hv) = reqwest::header::HeaderValue::from_str(v) {
                    req = req.header(hn, hv);
                }
            }
        }
        req = req.body(body.to_string());
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let text = resp.text().await?;
        Ok((status, text))
    }

    pub async fn post_text(&self, url: &str, body: &str) -> Result<String> {
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.to_string())
            .send()
            .await?;
        let text = resp.text().await?;
        Ok(text)
    }
}

#[derive(Clone)]
pub struct FlareSolverrClient {
    client: Client,
    endpoint: String,
}

#[derive(Serialize)]
struct FlareRequest<'a> {
    cmd: &'a str,
    url: &'a str,
    #[serde(rename = "maxTimeout")]
    max_timeout: u32,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FlareSolution {
    response: String,
    status: u16,
}

#[derive(Deserialize)]
struct FlareResponse {
    status: String,
    message: String,
    solution: Option<FlareSolution>,
}

impl FlareSolverrClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.into(),
        }
    }

    pub async fn resolve_html(&self, target_url: &str) -> Result<String> {
        let req_body = FlareRequest {
            cmd: "request.get",
            url: target_url,
            max_timeout: 60000,
        };

        let resp: FlareResponse = self
            .client
            .post(&self.endpoint)
            .json(&req_body)
            .send()
            .await?
            .json()
            .await?;

        if resp.status == "ok" {
            if let Some(sol) = resp.solution {
                return Ok(sol.response);
            }
        }

        Err(NetError::FlareSolverr(resp.message))
    }
}
