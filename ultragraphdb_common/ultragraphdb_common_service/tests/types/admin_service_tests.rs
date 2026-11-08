/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncReadExt, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_service::{AdminService, Bootstrap, RoleKind};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_http_server::{
    BearerKey, HttpLimits, HttpRequest, HttpResponse, HttpService, serve_http,
};

/// The admin key, and its SHA-256 digest by `shasum -a 256`.
const ADMIN_KEY: &str = "0123456789abcdef0123456789abcdef";
const ADMIN_DIGEST: &str = "3eb1bd439947eb762998e566ccc2e099c791118b2f40579cc4f7da2b5061b7f9";

/// Far past what a request on this host takes.
const WAIT: Duration = Duration::from_secs(5);

/// A config service refusing every request with 404, as it refuses a revision it does not hold.
#[derive(Clone)]
struct Refusing;

impl HttpService for Refusing {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        HttpResponse::new(404)
    }
}

/// Serves `service` on a free port of this runtime: the port.
async fn serving<H: HttpService + Clone + 'static>(service: H) -> TestResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            serve_http(stream, &HttpLimits::new(64, WAIT), &service).await;
        }
    })
    .detach();
    Ok(port)
}

/// A storage role's bootstrap asking the config service at `port`.
fn bootstrap(port: u16) -> TestResult<Bootstrap> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let key = std::path::Path::new(&root).join(format!("admin-{}-{port}", std::process::id()));
    std::fs::write(&key, "a read key of at least thirty two bytes")?;
    Ok(Bootstrap::read(
        RoleKind::Storage,
        [
            (
                "ULTRAGRAPHDB_CONFIG_URL",
                format!("http://127.0.0.1:{port}"),
            ),
            ("ULTRAGRAPHDB_CONFIG_TARGET", "ci".to_string()),
            ("ULTRAGRAPHDB_CONFIG_INSTANCE", "storage_1".to_string()),
            ("ULTRAGRAPHDB_CONFIG_KEY_FILE", key.display().to_string()),
        ]
        .map(|(name, value)| (name.to_string(), value)),
    )?)
}

/// An admin service fetching through the config service at `config`, with `loading` its revision under load: its port.
async fn admin(config: u16, loading: Option<u64>) -> TestResult<u16> {
    let key = BearerKey::from_digest_hex(ADMIN_DIGEST)?;
    serving(AdminService::new(
        key,
        bootstrap(config)?,
        Rc::new(Cell::new(loading)),
    ))
    .await
}

/// The status the service at `port` answers `head` presenting `key` with `body`.
async fn status(port: u16, head: &str, key: Option<&str>, body: &str) -> TestResult<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
    let bearer = key.map_or(String::new(), |key| {
        format!("Authorization: Bearer {key}\r\n")
    });
    let request = format!(
        "{head} HTTP/1.1\r\n{bearer}Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let BufResult(written, _) = stream.write_all(request.into_bytes()).await;
    written?;
    let read = compio::time::timeout(WAIT, stream.read_to_end(Vec::new()))
        .await
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?;
    let BufResult(read, bytes) = read;
    let _count: usize = read?;
    let reply = String::from_utf8(bytes)?;
    Ok(reply.split(' ').nth(1).context("a status")?.to_string())
}

/// What is no load, a load without the key, one naming no revision and one while another loads are refused before
/// anything is fetched.
#[test]
fn test_a_request_that_is_no_load_is_refused_before_a_fetch() -> TestResult {
    Runtime::new()?.block_on(async {
        let config = serving(Refusing).await?;
        let idle = admin(config, None).await?;
        let load = "POST /admin/load-config";
        assert_eq!(
            status(idle, "GET /admin/load-config", Some(ADMIN_KEY), "3").await?,
            "404"
        );
        assert_eq!(
            status(idle, "POST /admin", Some(ADMIN_KEY), "3").await?,
            "404"
        );
        assert_eq!(status(idle, load, None, "3").await?, "401");
        assert_eq!(status(idle, load, Some(ADMIN_KEY), "three").await?, "400");
        let busy = admin(config, Some(2)).await?;
        assert_eq!(status(busy, load, Some(ADMIN_KEY), "3").await?, "409");
        Ok(())
    })
}

/// A revision the config service refuses is 422 and one it cannot be asked for is 503; the role runs on either way.
#[test]
fn test_a_revision_that_cannot_be_fetched_leaves_the_role_running() -> TestResult {
    Runtime::new()?.block_on(async {
        let load = "POST /admin/load-config";
        let refused = admin(serving(Refusing).await?, None).await?;
        assert_eq!(status(refused, load, Some(ADMIN_KEY), "3").await?, "422");
        let closed = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let unreachable = admin(closed, None).await?;
        assert_eq!(
            status(unreachable, load, Some(ADMIN_KEY), "3").await?,
            "503"
        );
        Ok(())
    })
}
