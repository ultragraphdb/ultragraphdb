/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! quad-reads, "Compute runs each read on one core": a read-write transaction's Bind stream and its reads' Scans leave on the
//! core's one link to the partition (design D2), the partition played by a double whose Scans are answered from the table of
//! a played partition holding the transaction's own write.

use std::cell::Cell;
use std::rc::Rc;

use cucumber::{then, when};
use ultragraphdb_protocol_wire::{BindView, BodyView, Command, ScanView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::Steps;
use crate::apart::apart_partition::PlayedPartition;
use crate::apart::apart_reads::hold_subject;
use crate::endpoint_stage::{OBJECT, Options, PREDICATE, read_scans, rows_held, view_frame};
use crate::endpoint_steps_requests::run;

#[when("a read-write transaction writes and then reads")]
fn a_transaction_writes_and_then_reads(w: &mut Steps) -> TestResult {
    run(w, Options::default(), |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        // The partition's table holds the statement the transaction writes, as its own write reads back to it.
        let played = PlayedPartition::new(1);
        hold_subject(&played, 0, (&subject, PREDICATE), &[OBJECT], 100)?;
        let partition = stage
            .partition_with(1, rows_held(&played, &Rc::new(Cell::new(false))))
            .await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &subject).await?;
        let read = client.read(transaction, &subject).await?;
        let rows = client.got(read).await?;
        assert_eq!(rows.command, Command::Result, "{}", rows.message);
        assert!(
            String::from_utf8_lossy(&rows.bytes).contains(OBJECT),
            "the read returns the write"
        );
        assert_eq!(
            partition.accepted(),
            1,
            "one connection carried the Bind and the Scans"
        );
        let commands = partition.commands(&[Command::Credit, Command::Cancel]);
        let wrote = commands
            .iter()
            .position(|command| *command == Command::Write)
            .context("the Write")?;
        let scanned = commands
            .iter()
            .position(|command| *command == Command::Scan)
            .context("a Scan")?;
        assert_eq!(commands.first(), Some(&Command::Bind), "{commands:?}");
        assert!(
            wrote < scanned,
            "the read's Scans follow the write: {commands:?}"
        );
        let binds = partition.frames(Command::Bind);
        let bind = BindView::from_frame(&view_frame(binds.first().context("the Bind")?.bytes())?)?;
        for scan in read_scans(&partition) {
            let scan = ScanView::from_frame(&view_frame(scan.bytes())?)?;
            assert_eq!(
                scan.transaction(),
                bind.transaction(),
                "the Scan names the transaction"
            );
        }
        Ok(())
    })
}

#[then("the Bind stream and the read's Scans leave from one core, and the read returns the write")]
fn one_core_carries_both(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
