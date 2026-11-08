/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of one-frame writes, Drops and retried Commits relayed to partitions, and of what a partition's answer or its
//! connection's end makes of them.

use cucumber::{then, when};
use ultragraphdb_common_frontend::{DoubleFrame, partition_answer};
use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_wire::{
    Command, Commit, DirectWrite, DropCommand, ErrorCode, ErrorReply, TermTable,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::Steps;
use crate::endpoint_stage::{BIND_VERSION, CLUSTER, Options, Stage, insert_into, until};
use crate::endpoint_steps_requests::run;

/// The request a retried Commit names, and the last a partition says it took.
const RETRIED: u64 = 1;
const LAST_TAKEN: u64 = 3;

const GRAPH: &str = "http://ex.org/graph";

/// A partition answering a Recall with `refusal`, everything else as nothing goes wrong.
fn recalled(frame: &DoubleFrame, refusal: &ErrorReply) -> Answer {
    if frame.command() == Command::Recall {
        return Answer::Refuse(refusal.clone());
    }
    partition_answer(frame, CLUSTER, BIND_VERSION)
}

/// A retried Commit through a registered session on a stage whose partition 1 answers a Recall with `refusal`: the code and
/// detail it is refused with.
async fn retried(stage: &Stage, refusal: ErrorReply) -> TestResult<(ErrorCode, u64)> {
    let _partition = stage
        .partition_with(1, move |frame| recalled(frame, &refusal))
        .await?;
    let mut client = stage.ann().await?;
    let session = client.register().await?;
    let commit = client.send(&Commit::new(session, RETRIED, 0)).await?;
    client.refused(commit).await
}

#[when(
    "a client retries a Commit through its session, and the partition answers its Recall with code 14"
)]
fn a_busy_recall(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let busy = ErrorReply::new(ErrorCode::SessionBusy, "in its commit step");
        assert_eq!(retried(&stage, busy).await?.0, ErrorCode::SessionBusy);
        Ok(())
    })
}

#[then("the Commit is refused with code 14")]
fn a_busy_recall_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client retries a Commit through its session, and the partition answers its Recall with code 18 naming its last request"
)]
fn an_out_of_order_recall(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let later = ErrorReply::new(ErrorCode::RequestOutOfOrder, "a later request")
            .with_detail(LAST_TAKEN);
        assert_eq!(
            retried(&stage, later).await?,
            (ErrorCode::RequestOutOfOrder, LAST_TAKEN)
        );
        Ok(())
    })
}

#[then("the Commit is refused with code 18 naming that request")]
fn an_out_of_order_recall_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client retries a Commit through its session, and one of the two partitions has no process joined"
)]
fn a_recall_with_a_partition_unjoined(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let commit = client.send(&Commit::new(session, RETRIED, 0)).await?;
        assert_eq!(client.refused(commit).await?.0, ErrorCode::Unavailable);
        Ok(())
    })
}

#[then("the Commit is refused as unavailable")]
fn a_recall_with_a_partition_unjoined_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// A DirectWrite inserting `subject`'s statement, through no session.
fn direct(subject: &str) -> DirectWrite {
    let mut terms = TermTable::new();
    let change = insert_into(&mut terms, subject);
    DirectWrite::new(0, terms, vec![change])
}

#[when("partition 1 refuses a DirectWrite with code 26 naming partition 2 as the owner")]
fn a_one_frame_write_forced_elsewhere(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let owner: u32 = 2;
        let refusing = stage
            .partition_with(1, move |frame| {
                if frame.command() == Command::DirectWrite {
                    return Answer::Refuse(
                        ErrorReply::new(ErrorCode::WrongPartition, "another partition owns it")
                            .with_detail(u64::from(owner) << 32),
                    );
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let other = stage.partition(owner).await?;
        let mut client = stage.ann().await?;
        let write = client.send(&direct(&stage.on(1)?)).await?;
        let got = client.got(write).await?;
        assert_eq!(got.command, Command::Commit, "{}", got.message);
        assert_eq!(refusing.count(Command::DirectWrite), 1);
        assert_eq!(
            other.count(Command::DirectWrite),
            1,
            "resent once, to the owner named"
        );
        Ok(())
    })
}

#[then("the frontend resends it to partition 2, and its reply reaches the client")]
fn a_one_frame_write_forced_elsewhere_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a partition closes its connection while it holds a DirectWrite and a Drop's Bind")]
fn a_partition_closing_under_relays(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let holding = stage
            .partition_with(1, |frame| {
                let command = frame.command();
                if command == Command::DirectWrite || command == Command::Bind {
                    return Answer::Open;
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let mut client = stage.ann().await?;
        let write = client.send(&direct(&stage.on(1)?)).await?;
        let dropping = client.send(&DropCommand::tenant_data(0)).await?;
        until("both held", || {
            holding.count(Command::DirectWrite) == 1 && holding.count(Command::Bind) == 1
        })
        .await?;
        holding.close();
        for stream in [write, dropping] {
            assert_eq!(client.refused(stream).await?.0, ErrorCode::Unavailable);
        }
        Ok(())
    })
}

#[then("each is refused as unavailable")]
fn a_partition_closing_under_relays_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a client drops the whole tenant, a graph and the tenant's data")]
fn drops_of_every_scope(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let drops = [
            DropCommand::whole_tenant(0),
            DropCommand::graph(0, GRAPH),
            DropCommand::tenant_data(0),
        ];
        for drop_of in &drops {
            let stream = client.send(drop_of).await?;
            let got = client.got(stream).await?;
            assert_eq!(got.command, Command::Drop, "{}", got.message);
        }
        assert_eq!(partition.count(Command::Drop), drops.len());
        Ok(())
    })
}

#[then("each Drop is relayed to the partition, and its reply reaches the client")]
fn drops_of_every_scope_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
