/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of a transaction's declarations: a transaction holding them holds nothing else, and commits through no session.

use cucumber::{then, when};
use ultragraphdb_protocol_wire::{
    Commit, CommitOperation, Declared, DeclaredPredicate, ErrorCode, Operation, TermTable, Write,
    WriteChange,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::Steps;
use crate::endpoint_stage::{Client, Options, insert_into};
use crate::endpoint_steps_requests::{counter_terms, run};

const P: &str = "http://ex.org/declared";

/// A declaration of `p`, the Write's change at index 0.
fn declaration() -> WriteChange {
    WriteChange::Declare(Declared::Predicate(DeclaredPredicate::new(P)))
}

/// A Write declaring `p`.
fn declaring() -> Write {
    Write::new(TermTable::new(), vec![declaration()])
}

/// The code and detail of the Error ending `stream`.
async fn refused(client: &mut Client, stream: u64) -> TestResult<(ErrorCode, u64)> {
    client.refused(stream).await
}

#[when(
    "transactions declaring `p` go on with an Operation, a change of data or a read, one changes data and then declares, and one Write holds a declaration and a change of data"
)]
fn declarations_hold_nothing_else(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let _partition = stage.partition(1).await?;
        let subject = stage.on(1)?;
        let mut client = stage.ann().await?;
        let operation = client.begin(0).await?;
        client.on(operation, &declaring()).await?;
        let counter = CommitOperation::CounterAdd {
            graph: None,
            subject: 0,
            predicate: 1,
            delta: 1,
        };
        client
            .on(operation, &Operation::new(counter_terms(), counter))
            .await?;
        let data = client.begin(0).await?;
        client.on(data, &declaring()).await?;
        client.write(data, &subject).await?;
        let read = client.begin(0).await?;
        client.on(read, &declaring()).await?;
        let reading = client.read(read, &subject).await?;
        let changed = client.begin(0).await?;
        client.write(changed, &subject).await?;
        client.on(changed, &declaring()).await?;
        let mixed = client.begin(0).await?;
        let mut terms = TermTable::new();
        let insert = insert_into(&mut terms, &subject);
        let changes = vec![declaration(), insert];
        let first_data = changes
            .iter()
            .position(|change| !matches!(change, WriteChange::Declare(_)))
            .unwrap_or_default() as u64;
        client.on(mixed, &Write::new(terms, changes)).await?;
        for (stream, detail) in [(operation, 0), (data, 0), (changed, 0), (mixed, first_data)] {
            assert_eq!(
                refused(&mut client, stream).await?,
                (ErrorCode::Refused, detail)
            );
        }
        assert_eq!(refused(&mut client, reading).await?.0, ErrorCode::Refused);
        assert_eq!(refused(&mut client, read).await?.0, ErrorCode::Refused);
        Ok(())
    })
}

#[then(
    "each is refused with code 7, `detail` the index of the first change of data in its Write, and its transaction ends"
)]
fn declarations_hold_nothing_else_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a transaction declaring `p` commits through a session")]
fn declarations_commit_through_no_session(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let transaction = client.begin(0).await?;
        client.on(transaction, &declaring()).await?;
        client.on(transaction, &Commit::new(session, 1, 0)).await?;
        assert_eq!(
            refused(&mut client, transaction).await?.0,
            ErrorCode::Refused
        );
        client.silent(transaction, 5).await?;
        Ok(())
    })
}

#[then("the Commit is refused with code 7, and the transaction ends")]
fn declarations_commit_through_no_session_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
