/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;

use compio::net::TcpListener;
use compio::runtime::{Runtime, spawn};
use ultragraphdb_client_compio::{
    Anonymous, Client, ClientErrorEnum, TcpConnector, TlsConnector, TlsDial, TlsError, TlsTrust,
};
use ultragraphdb_protocol_transport_compio::CompioTlsStream;
use ultragraphdb_protocol_wire::{TermTable, Write, WriteChange};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};
use ultragraphdb_transport_tls::TestPki;

use crate::utils::serve::{config, serve, server};

fn runtime() -> TestResult<Runtime> {
    Runtime::new().context("a runtime")
}

/// A write as long as the largest frame's half: past rustls's 64 KiB, so it takes many records.
fn long_write() -> Write {
    let mut terms = TermTable::new();
    let a = terms.iri(&format!("http://ex.org/{}", "a".repeat(1 << 19)));
    Write::new(
        terms,
        vec![WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: a,
            object: a,
        }],
    )
}

/// Connections of `listener` accepted over TLS, the first served.
async fn accepted_once(
    listener: TcpListener,
    pki: TestPki,
    scripted: Rc<RefCell<ultragraphdb_client_core::TestServer>>,
) -> Result<(), TlsError> {
    let (stream, _peer) = listener
        .accept()
        .await
        .map_err(|e| TlsError::handshake_failed(String::new(), e.to_string()))?;
    let tls = CompioTlsStream::accept(&pki.acceptor()?, stream).await?;
    serve(&scripted, tls).await;
    Ok(())
}

/// A frame past what one TLS record holds, and past rustls's limit on what a session holds to write, crosses whole.
#[test]
fn test_a_frame_of_many_records_crosses_a_tls_connection() -> TestResult {
    let pki = TestPki::new()?;
    let trust = TlsTrust::from_pem("the test CA", pki.ca_pem().as_bytes())?;
    let scripted = server();
    let answer = runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let tcp = TcpConnector::new(listener.local_addr()?);
        let _server = spawn(accepted_once(listener, pki, Rc::clone(&scripted)));
        let connector = TlsDial::new(tcp, TlsConnector::new(trust)?, "localhost");
        let mut client = Client::new(connector, config(), Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        client.write(&tx, &long_write()).await?;
        let committed = client.commit(tx, session).await?;
        client.close().await;
        Ok::<_, TestError>(committed.request())
    })?;
    assert_eq!(answer, 1);
    let received = scripted.borrow();
    assert!(
        received
            .received()
            .iter()
            .any(|frame| frame.len() > 1 << 19)
    );
    Ok(())
}

/// A server's certificate no trust source vouches for fails the connection, naming the host, before any frame is read.
#[test]
fn test_a_certificate_of_another_authority_fails_the_connection() -> TestResult {
    let pki = TestPki::new()?;
    let other = TestPki::new()?;
    let trust = TlsTrust::from_pem("another CA", other.ca_pem().as_bytes())?;
    let scripted = server();
    let failed = runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let tcp = TcpConnector::new(listener.local_addr()?);
        let _server = spawn(accepted_once(listener, pki, Rc::clone(&scripted)));
        let connector = TlsDial::new(tcp, TlsConnector::new(trust)?, "localhost");
        let mut client = Client::new(connector, config(), Anonymous, 8);
        client.connect().await.err().context("refused")
    })?;
    assert!(
        matches!(failed.0, ClientErrorEnum::Transport { .. }),
        "{failed:?}"
    );
    assert!(failed.to_string().contains("localhost"), "{failed}");
    assert!(scripted.borrow().received().is_empty());
    Ok(())
}
