/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;

use compio::net::TcpListener;
use ultragraphdb_client_compio::{Anonymous, Client, ClientErrorEnum, Connect, TcpConnector};
use ultragraphdb_client_core::TestServer;
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};
use ultragraphdb_protocol_wire::{
    ColumnType, CommitOperation, Execute, Operation, PatternSlot, Prepare, QuadPattern, Read,
    TermTable, Write,
};

use crate::utils::serve::{SimConnect, config, serve, serve_sim, server};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

fn runtime() -> TestResult<compio::runtime::Runtime> {
    compio::runtime::Runtime::new().context("a runtime")
}

fn counter() -> Operation {
    let mut terms = TermTable::new();
    let (subject, predicate) = (terms.iri("http://ex.org/c"), terms.iri("http://ex.org/n"));
    Operation::new(
        terms,
        CommitOperation::CounterAdd {
            graph: None,
            subject,
            predicate,
            delta: 1,
        },
    )
}

/// A read of `tx`, naming it.
fn read(tx: &ultragraphdb_client_compio::Transaction) -> Read {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Read::new(
        tx.stream(),
        0,
        terms,
        QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(a),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        ),
        Vec::new(),
    )
}

/// A Prepare of the pattern `?s <http://ex.org/name> ?o` in the default graph, its subject a node parameter.
fn prepare_name() -> Prepare {
    let mut terms = TermTable::new();
    let name = terms.iri("http://ex.org/name");
    Prepare::new(
        0,
        0,
        0,
        terms,
        QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Parameter(0),
            PatternSlot::Bound(name),
            PatternSlot::Unbound,
        ),
        vec![ColumnType::Node],
        Vec::new(),
    )
}

/// An Execute of statement `id` in `tx`, its parameter `http://ex.org/a`.
fn execute(id: u64, tx: &ultragraphdb_client_compio::Transaction) -> Execute {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Execute::new(id, tx.stream(), 0, terms, vec![a], Vec::new())
}

/// A whole conversation: a session, a transaction with an operation committed, a read streamed under credit, a
/// statement prepared, executed, refused as stale and prepared again, a description, an abort.
async fn conversation<C: Connect>(
    client: &mut Client<C, Anonymous>,
    scripted: &RefCell<TestServer>,
) -> TestResult {
    let session = client.register().await.context("a session")?;
    let tx = client.begin(0, false).await.context("a transaction")?;
    client
        .write(&tx, &Write::new(TermTable::new(), Vec::new()))
        .await?;
    client.operation(&tx, &counter()).await?;
    let committed = client.commit(tx, session).await.context("committed")?;
    assert_eq!((committed.request(), committed.results().len()), (1, 1));
    let reader = client.begin(0, true).await?;
    let stream = client.read(&reader, &read(&reader)).await?;
    let mut rows = Vec::new();
    while let Some(texts) = client
        .next_batch(&stream, |v| {
            (0..v.rows())
                .filter_map(|r| v.term(v.cell(0, r)?)?.text().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .await?
    {
        rows.extend(texts);
    }
    assert_eq!(
        rows,
        vec!["http://ex.org/same".to_string(); 6],
        "three batches of two rows, under credit of one"
    );
    let prepare = prepare_name();
    let statement = client.prepare(&prepare).await?;
    scripted.borrow_mut().set_schema(7);
    let stale_tx = client.begin(0, true).await?;
    let stale = client
        .execute(&stale_tx, &execute(statement.id(), &stale_tx))
        .await?;
    let refused = client
        .next_batch(&stale, |_| ())
        .await
        .err()
        .context("an error")?;
    assert!(refused.is_stale(), "{refused:?}");
    let again = client.prepare(&prepare).await?;
    assert_eq!(again.schema(), 7);
    let again_tx = client.begin(0, true).await?;
    let executed = client
        .execute(&again_tx, &execute(again.id(), &again_tx))
        .await?;
    assert!(client.next_batch(&executed, |v| v.rows()).await?.is_some());
    client.cancel(executed).await;
    assert_eq!(client.describe(0).await?.read_version(), 1_000);
    let aborted = client.begin(0, false).await?;
    client.abort(aborted).await?;
    assert_eq!(scripted.borrow().applied(), 1);
    Ok(())
}

/// The Rust client: a lost reply retried through its session, applied once.
async fn retried<C: Connect>(
    client: &mut Client<C, Anonymous>,
    scripted: &RefCell<TestServer>,
) -> TestResult {
    let session = client.register().await?;
    let tx = client.begin(0, false).await?;
    let first = tx;
    let committed = client
        .commit(tx, session)
        .await
        .context("committed through the retry")?;
    assert_eq!(committed.request(), 1);
    assert_eq!(scripted.borrow().applied(), 1, "applied once");
    let second_tx = client.begin(0, false).await?;
    assert_eq!(client.commit(second_tx, session).await?.request(), 2);
    let stale = client
        .commit(first, session)
        .await
        .err()
        .context("an error")?;
    assert!(
        matches!(stale.0, ClientErrorEnum::StreamFinished { .. }),
        "the first connection's stream is gone: {stale:?}"
    );
    Ok(())
}

#[test]
fn test_a_conversation_over_the_simulated_network() -> TestResult {
    let rt = runtime()?;
    for faults in [
        NetworkFaults::none(),
        NetworkFaults::none()
            .with_split(500_000)
            .with_delay(300_000, 3),
    ] {
        let scripted = server();
        let connector = SimConnect::new(SimNetwork::new(3, faults));
        let accepted = Rc::clone(&connector.accepted);
        rt.block_on(async {
            let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
            let mut client = Client::new(connector, config(), Anonymous, 16);
            conversation(&mut client, &scripted).await?;
            client.close().await;
            Ok::<(), TestError>(())
        })?;
    }
    Ok(())
}

#[test]
fn test_a_lost_reply_is_retried_through_the_session_over_the_simulated_network() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    scripted.borrow_mut().drop_after_next_commit();
    scripted.borrow_mut().answer_busy_retries(1);
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 16);
        retried(&mut client, &scripted).await?;
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// Over TCP: the conversation, then a reply lost with its connection, retried through the session on a new one.
#[test]
fn test_the_client_runs_over_tcp_loopback() -> TestResult {
    let rt = runtime()?;
    for run_retry in [false, true] {
        let scripted = server();
        if run_retry {
            scripted.borrow_mut().drop_after_next_commit();
        }
        rt.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .context("a loopback port")?;
            let address = listener.local_addr()?;
            let serving = Rc::clone(&scripted);
            let _server = compio::runtime::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    serve(&serving, stream).await;
                }
            });
            let mut client = Client::new(TcpConnector::new(address), config(), Anonymous, 16);
            if run_retry {
                retried(&mut client, &scripted).await?;
            } else {
                conversation(&mut client, &scripted).await?;
            }
            client.close().await;
            Ok::<(), TestError>(())
        })?;
    }
    Ok(())
}

/// The shell tells the core the wall clock at each connect, so a credential's renewal falls due its margin before the
/// expiry the server states.
#[test]
#[allow(clippy::disallowed_methods)]
fn test_a_renewal_is_due_before_the_credentials_expiry() -> TestResult {
    use std::time::Duration;
    use ultragraphdb_client_core::{TEST_NAME, TEST_PASSWORD, TEST_PRINCIPAL};
    use ultragraphdb_protocol_scram::{MECHANISM, ScramLogin, system_nonce};
    let rt = runtime()?;
    let life = Duration::from_secs(600);
    let scripted = Rc::new(RefCell::new(
        TestServer::new(
            crate::utils::serve::CLUSTER,
            crate::utils::serve::welcome().with_mechanisms([MECHANISM]),
        )
        .with_credential_life(life),
    ));
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let login = ScramLogin::new(TEST_NAME, TEST_PASSWORD, system_nonce)?;
        let mut client = Client::new(connector, config(), login, 8);
        let before = std::time::Instant::now();
        client.connect().await?;
        let after = std::time::Instant::now();
        assert_eq!(client.core().principal(), Some(TEST_PRINCIPAL));
        let due = client.core().next_deadline().context("the renewal")?;
        assert!(
            due >= before + Duration::from_secs(530) && due <= after + Duration::from_secs(540),
            "60 s before the expiry"
        );
        Ok::<(), TestError>(())
    })?;
    Ok(())
}
