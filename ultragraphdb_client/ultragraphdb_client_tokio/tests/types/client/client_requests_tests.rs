/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use tokio::task::LocalSet;
use ultragraphdb_client_tokio::{Anonymous, Client};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};

use ultragraphdb_protocol_wire::{
    Begin, CommitOperation, ConditionalWrite, DirectWrite, DropCommand, ReadLevel, TermTable,
    WriteChange,
};

use crate::utils::serve::{SimConnect, config, in_doubt, serve_sim, server};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

fn runtime() -> TestResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("a runtime")
}

/// A commit left in doubt is resolved through the shell: the first attempt applied and its reply lost with the
/// connection, the second answered busy, so two attempts leave request 1 in doubt; the resolution answers it from the
/// server's record, committed once.
#[test]
fn test_a_commit_in_doubt_is_resolved_from_the_servers_record() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    scripted.borrow_mut().drop_after_next_commit();
    scripted.borrow_mut().answer_busy_retries(1);
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async {
        let _server: tokio::task::JoinHandle<()> =
            tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config().with_commit_attempts(2), Anonymous, 16);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        assert_eq!(
            in_doubt(&client.commit(tx, session).await.err().context("an error")?),
            Some((session.number(), 1))
        );
        let in_doubt_tx = client.begin(0, false).await?;
        assert_eq!(
            in_doubt(
                &client
                    .commit(in_doubt_tx, session)
                    .await
                    .err()
                    .context("an error")?
            ),
            Some((session.number(), 1))
        );
        let committed = client.resolve(session).await.context("resolved")?;
        assert_eq!(committed.request(), 1);
        assert_eq!(scripted.borrow().applied(), 1, "applied once");
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// The four request types through the shell: a one-frame write answered code 17 is in doubt, its resend commits it and
/// moves the counter; a ConditionalWrite takes the next number and carries the session's token; a Begin through the session
/// carries it too; a resumed session of no request is refused.
#[test]
fn test_the_request_types_through_the_shell() -> TestResult {
    let scripted = server();
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    let rt = runtime()?;
    LocalSet::new().block_on(&rt, async {
        let _server: tokio::task::JoinHandle<()> =
            tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 16);
        let session = client.register().await?;
        assert!(client.resume_session(session.number(), 0).is_err());
        scripted
            .borrow_mut()
            .refuse_next_commit(ultragraphdb_protocol_wire::ErrorCode::Unavailable, 0);
        let mut terms = TermTable::new();
        let a = terms.iri("http://ex.org/a");
        let write = DirectWrite::new(
            0,
            terms,
            vec![WriteChange::Insert {
                graph: None,
                subject: a,
                predicate: a,
                object: a,
            }],
        );
        let doubt = client
            .direct_write(write, session)
            .await
            .err()
            .context("in doubt")?;
        assert_eq!(in_doubt(&doubt), Some((session.number(), 1)));
        assert_eq!(client.resolve(session).await?.request(), 1);
        assert_eq!(client.core().next_request(session), Some(2));
        let mut counted = TermTable::new();
        let subject = counted.iri("http://ex.org/c");
        let predicate = counted.iri("http://ex.org/n");
        let counter = ConditionalWrite::new(
            0,
            counted,
            Vec::new(),
            vec![CommitOperation::CounterAdd {
                graph: None,
                subject,
                predicate,
                delta: 1,
            }],
        );
        let answer = client.conditional_write(counter, session).await?;
        assert_eq!(answer.request(), 2);
        assert_eq!(
            client.core().session_token(session).map(|t| t.epoch()),
            answer.token().map(|t| t.epoch())
        );
        let begin = Begin::new(0, 0, true).with_level(ReadLevel::MonotonicFast);
        let tx = client.begin_with(begin, session).await?;
        assert_eq!(tx.level(), ReadLevel::MonotonicFast);
        let dropped = client
            .drop_command(DropCommand::tenant_data(0), None)
            .await?;
        assert_eq!(dropped.request(), 0);
        let writer = client.begin(0, false).await?;
        let unsessioned = client.commit(writer, None).await?;
        assert_eq!(unsessioned.token(), None);
        client.close().await;
        Ok::<(), TestError>(())
    })?;
    Ok(())
}
