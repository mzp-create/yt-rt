use bytes::Bytes;
use futures::Stream;
use reqwest::{Client, ClientBuilder, Method, RequestBuilder, Response};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;
use yt_dlp_core::config::NetworkConfig;

use crate::cookies::Cookie;

pub struct HttpClient {
    inner: Client,
    default_headers: HashMap<String, String>,
}

impl HttpClient {
    pub fn new(config: &NetworkConfig) -> anyhow::Result<Self> {
        let mut builder = ClientBuilder::new()
            .cookie_store(true)
            .timeout(Duration::from_secs(config.socket_timeout.unwrap_or(30)))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
            .gzip(true)
            .brotli(true)
            .deflate(true);

        if let Some(ref proxy_url) = config.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy_url)?);
        }

        if config.force_ipv4 {
            builder = builder.local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        } else if config.force_ipv6 {
            builder = builder.local_address(std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED));
        }

        Ok(Self {
            inner: builder.build()?,
            default_headers: HashMap::new(),
        })
    }

    /// Builder method to set additional default headers.
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.default_headers.extend(headers);
        self
    }

    pub fn set_header(&mut self, key: String, value: String) {
        self.default_headers.insert(key, value);
    }

    /// Build a cookie header string from Cookie structs and add it to default headers.
    pub fn add_cookies(&self, cookies: &[Cookie]) -> String {
        cookies
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Create a request builder with the given method and URL, pre-applying default headers.
    pub fn request(&self, method: Method, url: &str) -> RequestBuilder {
        let mut req = self.inner.request(method, url);
        for (k, v) in &self.default_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req
    }

    // ── GET methods ──────────────────────────────────────────────────────

    pub async fn get(&self, url: &str) -> anyhow::Result<Response> {
        let req = self.request(Method::GET, url);
        Ok(req.send().await?)
    }

    pub async fn get_text(&self, url: &str) -> anyhow::Result<String> {
        Ok(self.get(url).await?.text().await?)
    }

    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> anyhow::Result<T> {
        let text = self.get_text(url).await?;
        let value = serde_json::from_str(&text)?;
        Ok(value)
    }

    pub async fn get_bytes(&self, url: &str) -> anyhow::Result<Bytes> {
        Ok(self.get(url).await?.bytes().await?)
    }

    /// Returns a streaming response body. Each item is a `Result<Bytes>`.
    pub async fn get_stream(
        &self,
        url: &str,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>> {
        let resp = self.get(url).await?;
        Ok(Box::pin(resp.bytes_stream()))
    }

    // ── POST methods ─────────────────────────────────────────────────────

    /// POST with a raw string body.
    pub async fn post(&self, url: &str, body: String) -> anyhow::Result<Response> {
        let req = self.request(Method::POST, url).body(body);
        Ok(req.send().await?)
    }

    /// POST with a JSON-serializable body.
    pub async fn post_json<T: serde::Serialize>(
        &self,
        url: &str,
        body: &T,
    ) -> anyhow::Result<Response> {
        let req = self.request(Method::POST, url).json(body);
        Ok(req.send().await?)
    }

    // ── HEAD method ──────────────────────────────────────────────────────

    /// Send a HEAD request, useful for checking content-length / resumability.
    pub async fn head(&self, url: &str) -> anyhow::Result<Response> {
        let req = self.request(Method::HEAD, url);
        Ok(req.send().await?)
    }

    // ── Accessors ────────────────────────────────────────────────────────

    pub fn inner(&self) -> &Client {
        &self.inner
    }
}
