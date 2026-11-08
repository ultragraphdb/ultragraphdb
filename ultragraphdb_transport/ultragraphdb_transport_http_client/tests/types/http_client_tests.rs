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

/// A service refusing a body it will not read answers a client still writing: the client reads the status, in clear text
/// and over TLS, and no failed write stands in for it.
#[test]
fn test_a_refusal_arrives_while_a_body_is_being_written() -> TestResult {
    let pki = TestPki::new()?;
    let body = vec![b'x'; HttpLimits::DRAIN_MAX / 2];
    Runtime::new()?.block_on(async {
        let plain = serving(None).await?;
        let in_clear = client(&format!("http://127.0.0.1:{plain}"), None, KEY)?
            .request("POST", "/", &body)
            .await?;
        assert_eq!(in_clear.status(), 413);
        let secure = serving(Some(pki.acceptor()?)).await?;
        let trust = TlsTrust::from_pem("test", pki.ca_pem().as_bytes())?;
        let over_tls = client(&format!("https://localhost:{secure}"), Some(trust), KEY)?
            .request("POST", "/", &body)
            .await?;
        assert_eq!(over_tls.status(), 413);
        Ok(())
    })
}

/// A peer that answers and closes over a body it never read fails the client's write; the answer waiting behind that
/// failure is the reply, and a peer that answers nothing leaves the failure itself to be told.
#[test]
fn test_a_reply_behind_a_failed_write_is_read() -> TestResult {
    use compio::BufResult;
    use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
    let body = vec![b'x'; 2 * HttpLimits::DRAIN_MAX];
    Runtime::new()?.block_on(async {
        for answer in [
            "HTTP/1.1 413 Content Too Large\r\nContent-Length: 0\r\n\r\n",
            "",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            spawn(async move {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let BufResult(_, _) = stream.read(Vec::with_capacity(1024)).await;
                    if !answer.is_empty() {
                        let BufResult(_, _) = stream.write_all(answer.as_bytes().to_vec()).await;
                    }
                    drop(stream.shutdown().await);
                }
            })
            .detach();
            let url = format!("http://{address}");
            let reached = client(&url, None, KEY)?.request("POST", "/", &body).await;
            if answer.is_empty() {
                let refused = reached.err().context("a refusal")?;
                assert!(refused.is_unreachable(), "{refused}");
                assert!(refused.to_string().contains(&url), "{refused}");
            } else {
                assert_eq!(reached?.status(), 413);
            }
        }
        Ok(())
    })
}

/// A peer that ends its side partway through a reply's body leaves the reply malformed at once: its end is no more bytes.
/// The peer runs on a thread of its own and resets the connection half a second after its end, so a client that went on
/// reading past the end, which a socket answers at once and forever, fails rather than holds the test.
#[test]
fn test_a_reply_ending_early_is_malformed() -> TestResult {
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    Runtime::new()?.block_on(async {
        compio::runtime::spawn_blocking(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                drop(stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nabc"));
                drop(stream.shutdown(std::net::Shutdown::Write));
                // Nothing of the request is read, so closing the socket resets the connection.
                std::thread::sleep(Duration::from_millis(500));
            }
        })
        .detach();
        let refused = client(&format!("http://{address}"), None, KEY)?
            .request("GET", "/", b"")
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(&refused.0, HttpClientErrorEnum::Malformed { reason, .. } if reason == "the reply ended early"),
            "{refused}"
        );
        Ok(())
    })
}

/// A client names the URL it was made for.
#[test]
fn test_a_client_names_its_url() -> TestResult {
    let url = HttpUrl::parse("http://h:1")?;
    let made = HttpClient::new(url.clone(), None, KEY.to_string())?;
    assert_eq!(made.url(), &url);
    Ok(())
}

/// A peer that accepts and never answers is timed out, which waiting may mend, naming the URL: the exchange's 5 s.
#[test]
fn test_a_silent_peer_is_timed_out() -> TestResult {
    Runtime::new()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        spawn(async move {
            // Held, unread and unanswered, until the runtime ends.
            let held = listener.accept().await;
            std::future::pending::<()>().await;
            drop(held);
        })
        .detach();
        let url = format!("http://{address}");
        let refused = client(&url, None, KEY)?
            .request("GET", "/", b"")
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(refused.0, HttpClientErrorEnum::TimedOut { .. }),
            "{refused}"
        );
        assert!(refused.is_unreachable());
        assert!(refused.to_string().contains(&url), "{refused}");
        Ok(())
    })
}
