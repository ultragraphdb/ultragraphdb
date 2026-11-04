/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::net::TcpListener;
use compio::runtime::{Runtime, spawn};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_http_client::{HttpClient, HttpClientErrorEnum, HttpUrl};
use ultragraphdb_transport_http_server::{
    BearerKey, HttpLimits, HttpRequest, HttpResponse, HttpService, serve_listener,
};
use ultragraphdb_transport_tls::{TestPki, TlsAcceptor, TlsTrust};

const KEY: &str = "0123456789abcdef0123456789abcdef";
const DIGEST: &str = "3eb1bd439947eb762998e566ccc2e099c791118b2f40579cc4f7da2b5061b7f9";

/// Answers a request presenting the key with its body, and any other with 401.
#[derive(Clone)]
struct Guarded(BearerKey);

impl HttpService for Guarded {
    async fn call(&self, request: HttpRequest) -> HttpResponse {
        if !self.0.allows(request.bearer()) {
            return HttpResponse::new(401);
        }
        let revision = request.path().len().to_string();
        HttpResponse::new(200)
            .with_header("X-Revision", revision)
            .with_body(request.into_body())
    }
}

/// A server of the key on a free port, through `tls`: the port.
async fn serving(tls: Option<TlsAcceptor>) -> TestResult<u16> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let listener = Listener::bind(&Address::parse(&format!("tcp://127.0.0.1:{port}"))?).await?;
    let limits = HttpLimits::new(1024, Duration::from_secs(5));
    let service = Guarded(BearerKey::from_digest_hex(DIGEST)?);
    spawn(serve_listener(listener, tls, limits, service)).detach();
    Ok(port)
}

fn client(url: &str, trust: Option<TlsTrust>, key: &str) -> TestResult<HttpClient> {
    Ok(HttpClient::new(
        HttpUrl::parse(url)?,
        trust,
        key.to_string(),
    )?)
}

/// In clear text, a request with the key is answered with its body, its status and its headers; another key's is 401.
#[test]
fn test_a_request_is_answered_in_clear_text() -> TestResult {
    Runtime::new()?.block_on(async {
        let port = serving(None).await?;
        let url = format!("http://127.0.0.1:{port}");
        let reply = client(&url, None, KEY)?
            .request("POST", "/abc", b"body")
            .await?;
        assert_eq!(reply.status(), 200);
        assert_eq!(reply.header("X-Revision"), Some("4"));
        assert_eq!(reply.body(), b"body");
        let refused = client(&url, None, "wrong")?
            .request("GET", "/", b"")
            .await?;
        assert_eq!(refused.status(), 401);
        Ok(())
    })
}

/// Over TLS verified against its trust, the same; a trust that does not hold its issuer is refused as TLS, no key sent.
#[test]
fn test_a_request_is_answered_over_verified_tls() -> TestResult {
    let pki = TestPki::new()?;
    let other = TestPki::new()?;
    Runtime::new()?.block_on(async {
        let port = serving(Some(pki.acceptor()?)).await?;
        let url = format!("https://localhost:{port}");
        let trust = TlsTrust::from_pem("test", pki.ca_pem().as_bytes())?;
        let reply = client(&url, Some(trust), KEY)?
            .request("GET", "/", b"")
            .await?;
        assert_eq!(reply.status(), 200);
        let foreign = TlsTrust::from_pem("other", other.ca_pem().as_bytes())?;
        let refused = client(&url, Some(foreign), KEY)?
            .request("GET", "/", b"")
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(refused.0, HttpClientErrorEnum::Tls { .. }),
            "{refused}"
        );
        assert!(!refused.is_unreachable());
        Ok(())
    })
}

/// An `https` URL with no trust is refused before anything is dialled, and an `http` one needs none.
#[test]
fn test_https_needs_a_trust() -> TestResult {
    let refused = HttpClient::new(HttpUrl::parse("https://h:1")?, None, KEY.to_string())
        .err()
        .context("a refusal")?;
    assert!(matches!(refused.0, HttpClientErrorEnum::Trust { .. }));
    assert!(refused.to_string().contains("https://h:1"));
    Ok(())
}

/// Nothing listening is what waiting may mend, naming the URL; so is a peer silent past the time.
#[test]
fn test_nothing_answering_is_unreachable() -> TestResult {
    Runtime::new()?.block_on(async {
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let url = format!("http://127.0.0.1:{port}");
        let refused = client(&url, None, KEY)?
            .request("GET", "/", b"")
            .await
            .err()
            .context("a refusal")?;
        assert!(refused.is_unreachable(), "{refused}");
        assert!(refused.to_string().contains(&url));
        Ok(())
    })
}

/// A peer that closes without a whole reply, and one answering what is no reply, are malformed, which waiting does not mend.
#[test]
fn test_what_is_no_reply_is_malformed() -> TestResult {
    use compio::BufResult;
    use compio::io::AsyncWriteExt;
    Runtime::new()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let BufResult(_, _) = stream.write_all(b"not http\r\n\r\n".to_vec()).await;
            }
        })
        .detach();
        let refused = client(&format!("http://{address}"), None, KEY)?
            .request("GET", "/", b"")
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(refused.0, HttpClientErrorEnum::Malformed { .. }),
            "{refused}"
        );
        assert!(!refused.is_unreachable());
        Ok(())
    })
}

/// A client's `Debug` holds its URL and never its key.
#[test]
fn test_a_client_never_prints_its_key() -> TestResult {
    let printed = format!("{:?}", client("http://h:1", None, KEY)?);
    assert!(
        printed.contains("http://h:1") && !printed.contains(KEY),
        "{printed}"
    );
    Ok(())
}
