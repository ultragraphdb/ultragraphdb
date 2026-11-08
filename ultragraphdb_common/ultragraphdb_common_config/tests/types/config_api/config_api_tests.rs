/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The HTTP API over a loopback socket, its store owned by a task on the same runtime, over the write-ahead log on its
//! simulated storage.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncReadExt, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_config::{CONFIG_LOG_CLUSTER, ConfigApi, ConfigStore, store_owner};
use ultragraphdb_common_config_check::TargetFiles;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};
use ultragraphdb_transport_http_server::{BearerKey, HttpLimits, serve_http};

const READ_KEY: &str = "read-key-0123456789abcdef01234567";
const ADMIN_KEY: &str = "admin-key-0123456789abcdef0123456";

const FRONTEND: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\npartition_count = 1\n[observability]\naddress = \"tcp://127.0.0.1:18081\"\n[frontend]\nrole_address = \"tcp://10.0.0.1:7081\"\ndata_dir = \"/data\"\n[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n";
const STORAGE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[storage]\npartition_count = 1\ndata_dir = \"/data\"\nrole_address = \"tcp://10.0.0.2:7000\"\n";

/// How long a client waits on an answer: far past what a loopback takes.
const WAIT: Duration = Duration::from_secs(5);

/// A key file holding `key`, in a folder no other test names.
fn key(name: &str, key: &str) -> TestResult<BearerKey> {
    let folder = std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!("config-api-{}", std::process::id()));
    std::fs::create_dir_all(&folder)?;
    let file = folder.join(name);
    std::fs::write(&file, key)?;
    Ok(BearerKey::read(&file.display().to_string())?)
}

fn bundle() -> Vec<u8> {
    let mut target = TargetFiles::new("ci");
    target.insert("frontend", "frontend.toml", FRONTEND.as_bytes().to_vec());
    target.insert("storage", "storage.toml", STORAGE.as_bytes().to_vec());
    target.to_bundle()
}

/// The API served at a loopback address on this runtime, its store empty.
async fn serving() -> TestResult<SocketAddr> {
    let config = WalConfig::new(CONFIG_LOG_CLUSTER, false);
    let (log, _) = Wal::open(SimStorage::new(9), PathBuf::from("/wal"), config).await?;
    let (handle, owner) = store_owner(ConfigStore::recover(log).await?);
    spawn(owner).detach();
    let api = ConfigApi::new(handle, key("read", READ_KEY)?, key("admin", ADMIN_KEY)?);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    spawn(async move {
        let limits = HttpLimits::new(64 * 1024, WAIT);
        while let Ok((stream, _)) = listener.accept().await {
            let api = api.clone();
            spawn(async move { serve_http(stream, &limits, &api).await }).detach();
        }
    })
    .detach();
    Ok(address)
}

/// The status and body of `method path` presenting `key`, with `body`.
async fn ask(
    address: SocketAddr,
    method: &str,
    path: &str,
    key: Option<&str>,
    body: &[u8],
) -> TestResult<(u16, Vec<u8>)> {
    let mut stream = TcpStream::connect(address).await?;
    let bearer = key.map_or(String::new(), |key| {
        format!("Authorization: Bearer {key}\r\n")
    });
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\n{bearer}Content-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(body);
    let BufResult(written, _) = stream.write_all(request).await;
    written?;
    let BufResult(read, answer) = compio::time::timeout(WAIT, stream.read_to_end(Vec::new()))
        .await
        .map_err(|_| TestError::new(format!("an answer to {method} {path}")))?;
    let _count: usize = read?;
    let end = answer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("the end of the head")?;
    let status = String::from_utf8_lossy(answer.get(9..12).context("a status")?).parse()?;
    Ok((status, answer.get(end + 4..).unwrap_or_default().to_vec()))
}

/// `/health` and `/ready` take no key; every other request takes one, and a change takes the admin key.
async fn keys(at: SocketAddr) -> TestResult {
    for path in ["/health", "/ready"] {
        assert_eq!(ask(at, "GET", path, None, b"").await?.0, 200, "{path}");
    }
    assert_eq!(ask(at, "GET", "/v1/targets", None, b"").await?.0, 401);
    assert_eq!(
        ask(at, "GET", "/v1/targets", Some(READ_KEY), b"").await?.0,
        200
    );
    let add = "/v1/targets/ci/revisions?base=0";
    assert_eq!(
        ask(at, "POST", add, Some(READ_KEY), &bundle()).await?.0,
        403
    );
    assert_eq!(
        ask(at, "POST", add, Some(ADMIN_KEY), &bundle()).await?.0,
        201
    );
    Ok(())
}

/// Each request reaches its own answer.
async fn routes(at: SocketAddr) -> TestResult {
    let read = Some(READ_KEY);
    let admin = Some(ADMIN_KEY);
    assert_eq!(
        ask(at, "PUT", "/v1/targets/ci/active", admin, b"1")
            .await?
            .0,
        200
    );
    let listed = ask(at, "GET", "/v1/targets", read, b"").await?;
    assert_eq!(listed, (200, b"ci latest 1 active 1\n".to_vec()));
    for path in [
        "/v1/targets/ci/instances/storage",
        "/v1/targets/ci/revisions/1/instances/storage",
    ] {
        let (status, body) = ask(at, "GET", path, read, b"").await?;
        assert_eq!(status, 200, "{path}");
        assert!(
            String::from_utf8(body)?.contains("partition_count = 1"),
            "{path}"
        );
    }
    for path in ["/v1/targets/ci/revisions/1", "/v1/targets/ci/download"] {
        assert_eq!(
            ask(at, "GET", path, admin, b"").await?,
            (200, bundle()),
            "{path}"
        );
    }
    assert_eq!(
        ask(at, "DELETE", "/v1/targets/ci", admin, b"").await?.0,
        200
    );
    let retired = ask(at, "GET", "/v1/targets", read, b"").await?;
    assert_eq!(retired, (200, b"ci latest 1 active 1 retired\n".to_vec()));
    Ok(())
}

/// The API answers each request by its key and its route, the store behind it.
#[test]
fn test_the_api_answers_each_request() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = serving().await?;
        keys(at).await?;
        routes(at).await
    })
}
