/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;

use tokio::net::TcpListener;
use tokio::task::LocalSet;
use ultragraphdb_client_core::TestServer;
use ultragraphdb_client_tokio::{Anonymous, Client, ClientErrorEnum, Connect, TcpConnector};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};
use ultragraphdb_protocol_wire::{
    ColumnType, CommitOperation, Execute, Operation, PatternSlot, Prepare, QuadPattern, Read,
    TermTable, Write,
};

use crate::utils::serve::{SimConnect, config, serve, serve_sim, server};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
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
fn read(tx: &ultragraphdb_client_tokio::Transaction) -> Read {
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

/// A whole conversation: a session, a transaction with an operation committed, a read streamed under credit, a
/// statement prepared, executed, refused as stale and prepared again, a description, an abort.
async fn conversation<C: Connect>(
    client: &mut Client<C, Anonymous>,
    scripted: &RefCell<TestServer>,
) {
    let session = client.register().await.expect("a session");
    let tx = client.begin(0, false).await.expect("a transaction");
    client
        .write(&tx, &Write::new(TermTable::new(), Vec::new()))
        .await
        .unwrap();
    client.operation(&tx, &counter()).await.unwrap();
    let committed = client.commit(tx, session).await.expect("committed");
    assert_eq!((committed.request(), committed.results().len()), (1, 1));
    let reader = client.begin(0, true).await.unwrap();
    let stream = client.read(&reader, &read(&reader)).await.unwrap();
    let mut rows = Vec::new();
    while let Some(texts) = client
        .next_batch(&stream, |v| {
            (0..v.rows())
                .filter_map(|r| v.term(v.cell(0, r)?)?.text().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap()
    {
        rows.extend(texts);
    }
    assert_eq!(
        rows,
        vec!["http://ex.org/same".to_string(); 6],
        "three batches of two rows, under credit of one"
    );
    let mut terms = TermTable::new();
    let name = terms.iri("http://ex.org/name");
    let prepare = Prepare::new(
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
    );
    let statement = client.prepare(&prepare).await.unwrap();
    let execute = |id| {
        let mut terms = TermTable::new();
        let a = terms.iri("http://ex.org/a");
        move |tx: &ultragraphdb_client_tokio::Transaction| {
            Execute::new(id, tx.stream(), 0, terms.clone(), vec![a], Vec::new())
        }
    };
    scripted.borrow_mut().set_schema(7);
    let tx = client.begin(0, true).await.unwrap();
    let stale = client
        .execute(&tx, &execute(statement.id())(&tx))
        .await
        .unwrap();
    let refused = client.next_batch(&stale, |_| ()).await.unwrap_err();
    assert!(refused.is_stale(), "{refused:?}");
    let again = client.prepare(&prepare).await.unwrap();
    assert_eq!(again.schema(), 7);
    let tx = client.begin(0, true).await.unwrap();
    let stream = client
        .execute(&tx, &execute(again.id())(&tx))
        .await
        .unwrap();
    assert!(
        client
            .next_batch(&stream, |v| v.rows())
            .await
            .unwrap()
            .is_some()
    );
    client.cancel(stream).await;
    assert_eq!(client.describe(0).await.unwrap().read_version(), 1_000);
    let tx = client.begin(0, false).await.unwrap();
    client.abort(tx).await.unwrap();
    assert_eq!(scripted.borrow().applied(), 1);
}

/// The Rust client: a lost reply retried through its session, applied once.
async fn retried<C: Connect>(client: &mut Client<C, Anonymous>, scripted: &RefCell<TestServer>) {
    let session = client.register().await.unwrap();
    let tx = client.begin(0, false).await.unwrap();
    let first = tx;
    let committed = client
        .commit(tx, session)
        .await
        .expect("committed through the retry");
    assert_eq!(committed.request(), 1);
    assert_eq!(scripted.borrow().applied(), 1, "applied once");
    let tx = client.begin(0, false).await.unwrap();
    assert_eq!(client.commit(tx, session).await.unwrap().request(), 2);
    let stale = client.commit(first, session).await.unwrap_err();
    assert!(
        matches!(stale.0, ClientErrorEnum::StreamFinished { .. }),
        "the first connection's stream is gone: {stale:?}"
    );
}

/// A transaction of a client whose connection is closed: a handle for the Send check.
fn unreachable_transaction() -> ultragraphdb_client_tokio::Transaction {
    let rt = runtime();
    let scripted = server();
    let network = SimNetwork::new(5, NetworkFaults::none());
    let connector = SimConnect::new(network);
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async move {
        tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut other = Client::new(connector, config(), Anonymous, 8);
        let tx = other.begin(0, false).await.unwrap();
        other.close().await;
        tx
    })
}

#[test]
fn test_a_conversation_over_the_simulated_network() {
    let rt = runtime();
    for faults in [
        NetworkFaults::none(),
        NetworkFaults::none()
            .with_split(500_000)
            .with_delay(300_000, 3),
    ] {
        let scripted = server();
        let connector = SimConnect::new(SimNetwork::new(3, faults));
        let accepted = Rc::clone(&connector.accepted);
        LocalSet::new().block_on(&rt, async {
            tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
            let mut client = Client::new(connector, config(), Anonymous, 16);
            conversation(&mut client, &scripted).await;
            client.close().await;
        });
    }
}

#[test]
fn test_a_lost_reply_is_retried_through_the_session_over_the_simulated_network() {
    let rt = runtime();
    let scripted = server();
    scripted.borrow_mut().drop_after_next_commit();
    scripted.borrow_mut().answer_busy_retries(1);
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async {
        tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 16);
        retried(&mut client, &scripted).await;
    });
}

/// Over TCP: the conversation, then a reply lost with its connection, retried through the session on a new one.
#[test]
fn test_the_client_runs_over_tcp_loopback() {
    let rt = runtime();
    for run_retry in [false, true] {
        let scripted = server();
        if run_retry {
            scripted.borrow_mut().drop_after_next_commit();
        }
        LocalSet::new().block_on(&rt, async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("a loopback port");
            let address = listener.local_addr().unwrap();
            let serving = Rc::clone(&scripted);
            tokio::task::spawn_local(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    serve(&serving, stream).await;
                }
            });
            let mut client = Client::new(TcpConnector::new(address), config(), Anonymous, 16);
            if run_retry {
                retried(&mut client, &scripted).await;
            } else {
                conversation(&mut client, &scripted).await;
            }
            client.close().await;
        });
    }
}

/// Over TCP the client's futures are `Send`: it runs on a multi-threaded runtime.
#[test]
fn test_the_tcp_clients_futures_are_send() {
    fn send<T: Send>(_: &T) {}
    let mut client = Client::new(
        TcpConnector::new("127.0.0.1:9".parse().unwrap()),
        config(),
        Anonymous,
        2,
    );
    let tx = unreachable_transaction();
    let session = client.resume_session(1, 1);
    send(&client.register());
    send(&client.commit(tx, session));
    send(&client.next_batch(&result_stream(), |v| v.rows()));
}

/// A result stream of another client: a handle for the Send check.
fn result_stream() -> ultragraphdb_client_tokio::ResultStream {
    let rt = runtime();
    let scripted = server();
    let connector = SimConnect::new(SimNetwork::new(5, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async move {
        tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut other = Client::new(connector, config(), Anonymous, 8);
        let tx = other.begin(0, true).await.unwrap();
        other.read(&tx, &read(&tx)).await.unwrap()
    })
}

/// The shell tells the core the wall clock at each connect, so a credential's renewal falls due its margin before the
/// expiry the server states.
#[test]
#[allow(clippy::disallowed_methods)]
fn test_a_renewal_is_due_before_the_credentials_expiry() {
    use std::time::Duration;
    use ultragraphdb_client_core::{TEST_NAME, TEST_PASSWORD, TEST_PRINCIPAL};
    use ultragraphdb_protocol_scram::{MECHANISM, ScramLogin, system_nonce};
    let rt = runtime();
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
    LocalSet::new().block_on(&rt, async {
        tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let login = ScramLogin::new(TEST_NAME, TEST_PASSWORD, system_nonce).unwrap();
        let mut client = Client::new(connector, config(), login, 8);
        let before = tokio::time::Instant::now().into_std();
        client.connect().await.unwrap();
        let after = tokio::time::Instant::now().into_std();
        assert_eq!(client.core().principal(), Some(TEST_PRINCIPAL));
        let due = client.core().next_deadline().expect("the renewal");
        assert!(
            due >= before + Duration::from_secs(530) && due <= after + Duration::from_secs(540),
            "60 s before the expiry"
        );
    });
}
