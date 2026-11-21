use reqwest::{header, Client, Response};
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, sync::Arc, time::Duration};
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
    public_only: bool,
    flaresolverr_url: Option<String>,
}

fn public_url_allowed(url: &url::Url) -> bool {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) => is_safe_ip(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => is_safe_ip(IpAddr::V6(ip)),
        Some(url::Host::Domain(domain)) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            !(domain == "localhost"
                || domain.ends_with(".localhost")
                || domain.ends_with(".local")
                || domain.ends_with(".internal")
                || domain.ends_with(".lan"))
        }
        None => false,
    }
}

pub fn is_public_url(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|url| public_url_allowed(&url))
}

struct PublicDnsResolver;

fn public_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if public_url_allowed(attempt.url()) {
            attempt.follow()
        } else {
            attempt.error("redirect to a private or invalid destination")
        }
    })
}

impl reqwest::dns::Resolve for PublicDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if addresses.is_empty() || addresses.iter().any(|addr| !is_safe_ip(addr.ip())) {
                return Err(std::io::Error::other("DNS resolved to a non-public address").into());
            }
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

impl HttpClient {
    pub fn with_flaresolverr_url(mut self, url: Option<String>) -> Self {
        self.flaresolverr_url = url;
        self
    }

    pub fn is_public_only(&self) -> bool {
        self.public_only
    }

    fn validate_destination(&self, raw: &str) -> Result<()> {
        if self.public_only {
            let parsed = url::Url::parse(raw)
                .map_err(|e| NetError::SsrfBlocked(format!("Invalid URL: {e}")))?;
            if !public_url_allowed(&parsed) {
                return Err(NetError::SsrfBlocked("Forbidden destination".into()));
            }
        }
        Ok(())
    }

    pub fn new() -> Result<Self> {
        Self::build(false)
    }

    pub fn new_public_only() -> Result<Self> {
        Self::build(true)
    }

    fn build(public_only: bool) -> Result<Self> {
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

        let mut builder = Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .timeout(Duration::from_secs(30));
        if public_only {
            builder = builder
                .no_proxy()
                .dns_resolver(Arc::new(PublicDnsResolver))
                .redirect(public_redirect_policy());
        }
        let client = builder.build()?;

        Ok(Self {
            client,
            public_only,
            flaresolverr_url: None,
        })
    }

    pub async fn get(&self, url: &str) -> Result<Response> {
        self.validate_destination(url)?;
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
        self.validate_destination(url)?;
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
        if !self.public_only && (status == 403 || status == 503) {
            let fs_url = self
                .flaresolverr_url
                .clone()
                .or_else(|| std::env::var("FLARESOLVERR_URL").ok());
            if let Some(fs_url) = fs_url {
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
        self.validate_destination(url)?;
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
        self.validate_destination(url)?;
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
        self.validate_destination(url)?;
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

#[cfg(test)]
mod public_client_tests {
    use super::*;

    #[tokio::test]
    async fn public_transport_rejects_private_destinations_and_private_dns() {
        let client = HttpClient::new_public_only().unwrap();
        let headers = std::collections::HashMap::new();
        for url in [
            "http://127.0.0.1:8080/",
            "http://[::1]:8080/",
            "http://localhost./",
            "http://169.254.169.254/",
            "http://[::127.0.0.1]/",
        ] {
            assert!(
                client
                    .get_bytes_and_status_with_headers(url, &headers)
                    .await
                    .is_err(),
                "{url}"
            );
            assert!(
                client
                    .post_text_with_headers(url, "", None, &headers)
                    .await
                    .is_err(),
                "{url}"
            );
        }
        let resolver = PublicDnsResolver;
        use reqwest::dns::Resolve;
        assert!(resolver
            .resolve("localhost".parse().unwrap())
            .await
            .is_err());
        assert!(!is_public_url("http://127.0.0.1/redirect"));
        assert!(is_public_url("https://example.com/"));
    }

    #[tokio::test]
    async fn redirects_to_private_services_are_blocked() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin_addr = origin.local_addr().unwrap();
        let target_addr = target.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = origin.accept().await.unwrap();
            let mut request = [0; 1024];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 302 Found\r\nLocation: http://{target_addr}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .resolve("public.test", origin_addr)
            .redirect(public_redirect_policy())
            .build()
            .unwrap();
        assert!(client
            .get(format!("http://public.test:{}/", origin_addr.port()))
            .send()
            .await
            .is_err());
        server.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), target.accept())
                .await
                .is_err()
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_flaresolverr_client_success_error_and_missing_solution() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/ok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok",
                "message": "",
                "solution": { "response": "<html>solved</html>", "status": 200 }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/err"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "error",
                "message": "Challenge failed",
                "solution": null
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/empty"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok",
                "message": "missing solution",
                "solution": null
            })))
            .mount(&server)
            .await;

        let ok_client = FlareSolverrClient::new(format!("{}/ok", server.uri()));
        assert_eq!(
            ok_client
                .resolve_html("https://target.example")
                .await
                .unwrap(),
            "<html>solved</html>"
        );

        let err_client = FlareSolverrClient::new(format!("{}/err", server.uri()));
        let err = err_client
            .resolve_html("https://target.example")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Challenge failed"));

        let empty_client = FlareSolverrClient::new(format!("{}/empty", server.uri()));
        let empty_err = empty_client
            .resolve_html("https://target.example")
            .await
            .unwrap_err();
        assert!(empty_err.to_string().contains("missing solution"));
    }

    #[tokio::test]
    async fn test_http_client_headers_post_errors_and_flaresolverr_fallback() {
        let origin = MockServer::start().await;
        let flare = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/blocked-403"))
            .and(header("X-Custom", "val1"))
            .respond_with(ResponseTemplate::new(403).set_body_string("cf block"))
            .mount(&origin)
            .await;

        Mock::given(method("GET"))
            .and(path("/blocked-503"))
            .respond_with(ResponseTemplate::new(503).set_body_string("cf challenge"))
            .mount(&origin)
            .await;

        Mock::given(method("GET"))
            .and(path("/not-found"))
            .respond_with(ResponseTemplate::new(404).set_body_string("nope"))
            .mount(&origin)
            .await;

        Mock::given(method("POST"))
            .and(path("/post-default"))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string("a=1&b=2"))
            .respond_with(ResponseTemplate::new(200).set_body_string("form-ok"))
            .mount(&origin)
            .await;

        Mock::given(method("POST"))
            .and(path("/post-json"))
            .and(header("content-type", "application/json"))
            .and(header("X-Auth", "tok"))
            .and(body_string(r#"{"x":1}"#))
            .respond_with(ResponseTemplate::new(201).set_body_string("json-ok"))
            .mount(&origin)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok",
                "message": "",
                "solution": { "response": "<html>bypassed</html>", "status": 200 }
            })))
            .mount(&flare)
            .await;

        let client = HttpClient::new()
            .unwrap()
            .with_flaresolverr_url(Some(format!("{}/v1", flare.uri())));
        let mut hdrs = HashMap::new();
        hdrs.insert("X-Custom".to_string(), "val1".to_string());

        let (st403, body403) = client
            .get_text_with_headers(&format!("{}/blocked-403", origin.uri()), &hdrs)
            .await
            .unwrap();
        assert_eq!(st403, 200);
        assert_eq!(body403, "<html>bypassed</html>");

        let (st503, body503) = client
            .get_text_with_headers(&format!("{}/blocked-503", origin.uri()), &HashMap::new())
            .await
            .unwrap();
        assert_eq!(st503, 200);
        assert_eq!(body503, "<html>bypassed</html>");

        let bytes_err = client
            .get_bytes_with_headers(&format!("{}/not-found", origin.uri()), &HashMap::new())
            .await
            .unwrap_err();
        assert!(bytes_err.to_string().contains("404"));

        let (p1_st, p1_body) = client
            .post_text_with_headers(
                &format!("{}/post-default", origin.uri()),
                "a=1&b=2",
                None,
                &HashMap::new(),
            )
            .await
            .unwrap();
        assert_eq!(p1_st, 200);
        assert_eq!(p1_body, "form-ok");

        let mut json_hdrs = HashMap::new();
        json_hdrs.insert("X-Auth".to_string(), "tok".to_string());
        let (p2_st, p2_body) = client
            .post_text_with_headers(
                &format!("{}/post-json", origin.uri()),
                r#"{"x":1}"#,
                Some("application/json"),
                &json_hdrs,
            )
            .await
            .unwrap();
        assert_eq!(p2_st, 201);
        assert_eq!(p2_body, "json-ok");

        // public_only client rejects loopback and never triggers FlareSolverr
        let public_client = HttpClient::new_public_only()
            .unwrap()
            .with_flaresolverr_url(Some(format!("{}/v1", flare.uri())));
        assert!(public_client
            .get_text_with_headers(&format!("{}/blocked-403", origin.uri()), &hdrs)
            .await
            .is_err());
    }

    #[test]
    fn test_is_public_url_filtering() {
        assert!(!is_public_url("https://user:pass@example.com/"));
        assert!(!is_public_url("https://user@example.com/"));
        assert!(!is_public_url("http://service.local/api"));
        assert!(!is_public_url("http://db.internal/"));
        assert!(!is_public_url("http://printer.lan/"));
        assert!(!is_public_url("ftp://example.com/file"));
        assert!(!is_public_url("javascript:alert(1)"));
        assert!(is_public_url("http://example.com/path"));
        assert!(is_public_url("https://example.com/path"));
    }

    #[tokio::test]
    async fn test_public_redirect_policy_rejects_over_10_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            for _ in 0..12 {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://public.test:{}/next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    addr.port()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });

        let client = reqwest::Client::builder()
            .no_proxy()
            .resolve("public.test", addr)
            .redirect(public_redirect_policy())
            .build()
            .unwrap();

        let err = client
            .get(format!("http://public.test:{}/start", addr.port()))
            .send()
            .await
            .unwrap_err();
        assert!(err.is_redirect() || err.to_string().contains("redirect"));
        server.abort();
    }
}
