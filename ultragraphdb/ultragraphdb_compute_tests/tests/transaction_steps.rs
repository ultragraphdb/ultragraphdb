/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read of a read-write transaction: a Scan naming it, seeing its writes and recording what its commit validates.

use cucumber::{then, when};
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_protocol_wire::{
    Cancel, Command, Commit, ErrorCode, ReadLevel, TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::term_key;
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::client::{self, EndpointClient};
use crate::played_partition::Script;
use crate::scene::{
    KNOWS, NAME, Pattern, Scene, insert_of, iri, reads, refusal_of, run, string, until,
};
use crate::{Steps, played, played_tables, solo};

/// How long a stream stays silent for a scenario to take it as ended.
const SILENCE: std::time::Duration = std::time::Duration::from_millis(200);

/// Conflict kind Range (`PROTOCOL.md` §9.4).
const KIND_RANGE: u64 = 3;

/// A write of `<unrelated> <name> "x"`, for a transaction reading and then writing.
fn unrelated(scene: &Scene) -> TestResult<Write> {
    insert_of(None, &scene.iri_in(1, "unrelated")?, NAME, &string("x"))
}

/// Writes something unrelated in the client's transaction on `transaction`, then commits: its refusal's conflict kind, `None`
/// for a commit that holds. A frame on the read's stream `read` fails it.
async fn commit_a_write(
    scene: &Scene,
    client: &mut EndpointClient,
    (transaction, read): (u64, u64),
) -> TestResult<Option<u64>> {
    client.send_on(transaction, &unrelated(scene)?).await?;
    client
        .send_on(transaction, &Commit::new([0; 16], 0, 0))
        .await?;
    loop {
        let frame = client.next_frame("the answer to the Commit").await?;
        let stream = frame.view()?.header().stream();
        if stream == read {
            return Err(TestError::new("a frame of the read reached the client"));
        }
        if stream != transaction {
            continue;
        }
        if let Some(refused) = refusal_of(&frame)? {
            assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
            return Ok(Some(refused.detail));
        }
        let command = frame.view()?.command();
        return if command == Command::Commit {
            Ok(None)
        } else {
            Err(TestError::new(format!("the Commit answered {command:?}")))
        };
    }
}

#[when("a transaction scans 10,000 lists through compute")]
fn ten_thousand(w: &mut Steps) -> TestResult {
    const LISTS: u32 = 10_000;
    let scene = Scene::new(solo()?);
    run(async {
        for chunk in (0..LISTS).collect::<Vec<u32>>().chunks(2_000) {
            let mut table = TermTable::new();
            let predicate = table.iri(NAME);
            let object = table.intern(string("x"))?;
            let changes = chunk
                .iter()
                .map(|at| WriteChange::Insert {
                    graph: None,
                    subject: table.iri(&format!("http://ex.org/s{at}")),
                    predicate,
                    object,
                })
                .collect();
            scene.commit(1, &Write::new(table, changes)).await?;
        }
        scene.info.log.clear();
        let read = client::read_at(
            &scene,
            &Pattern::of(None, Some(NAME), None),
            false,
            ReadLevel::Snapshot,
        )?;
        w.answers = vec![client::answer(&scene, &read).await?];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "its read set holds one range and no list key, and it holds none of those lists in memory afterwards"
)]
fn one_range(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.rows().len(), 10_000);
    let scene = w.scene()?;
    let lists: Vec<_> = scene
        .info
        .log
        .scans()
        .into_iter()
        .filter(|scan| reads(scan, Keyspace::Forward))
        .collect();
    let [scan] = lists.as_slice() else {
        return Err(TestError::new(format!("{lists:?}")));
    };
    assert_eq!(scan.ranges.len(), 1, "one range");
    let frames = scene.info.log.versions().len();
    assert!(frames < 10_000, "lists answered by whole frames: {frames}");
    Ok(())
}

#[when(
    "a Commit cancels a read whose Scan records one more span before the Cancel reaches the partition, and another transaction wrote into that span"
)]
fn span_after_cancel(w: &mut Steps) -> TestResult {
    let scene = Scene::new(solo()?);
    run(async {
        let (a, b, c) = (
            scene.iri_in(1, "a")?,
            scene.iri_in(1, "b")?,
            scene.iri_in(1, "c")?,
        );
        scene
            .commit(1, &insert_of(None, &a, NAME, &string("y"))?)
            .await?;
        scene
            .commit(1, &insert_of(None, &c, KNOWS, &iri(&b))?)
            .await?;
        let log = &scene.info.log;
        log.clear();
        // The dictionary's Scan goes on; the Scan of `<a>`'s list, the one more span, waits at the tap for compute's Cancel of it.
        log.pause_until_next_frame(1);
        let read = client::read_at(
            &scene,
            &Pattern::of(Some(&a), Some(KNOWS), None),
            false,
            ReadLevel::Snapshot,
        )?;
        let mut client = EndpointClient::connect(scene.info, read.max_frame).await?;
        let transaction = match client.begin(&read.begin).await? {
            Ok(transaction) => transaction,
            Err(refused) => return Err(TestError::new(format!("the Begin: {refused:?}"))),
        };
        let stream = client.send_read(transaction, &read).await?;
        until("two Scans seen", || log.scans().len() >= 2).await?;
        assert_eq!(log.scans().len(), 2, "the list's Scan held");
        // The Cancel a Commit has compute send. A Commit's own Cancel goes on the reads' connection and its Commit on the Bind
        // stream's, which the tap does not order, so the client's Cancel of the read stands for it, the Commit following.
        client.send_on(stream, &Cancel).await?;
        // The tap lets compute's Cancel go on once the partition has replied to the Scan held, so the Scan has run before it.
        until("a Cancel seen", || log.cancels() >= 1).await?;
        log.go_on();
        assert_eq!(log.cancels(), 1, "compute's Cancel of its Scan");
        scene
            .commit(1, &insert_of(None, &a, KNOWS, &iri(&b))?)
            .await?;
        w.kind = commit_a_write(&scene, &mut client, (transaction, stream)).await?;
        let late = compio::time::timeout(SILENCE, client.next_frame("a late frame")).await;
        assert!(late.is_err(), "a frame reached the client after its Cancel");
        Ok(())
    })
}

#[then(
    "the commit aborts with a conflict of kind Range, and no frame of that span reached the client"
)]
fn aborts_range_unseen(w: &mut Steps) {
    assert_eq!(w.kind, Some(KIND_RANGE));
}

#[when("a transaction's reads leave no room for a span")]
fn no_room(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played()?);
    let subject = scene.iri_in(1, "a")?;
    played_tables(1)?.script(
        term_key(scene.tenant, &subject),
        Script::Refuse(ErrorCode::TooLarge, 524_288),
    );
    run(async {
        let pattern = Pattern::of(Some(&subject), Some(KNOWS), None);
        let read = client::read_at(&scene, &pattern, false, ReadLevel::Snapshot)?;
        w.answers = vec![client::answer(&scene, &read).await?];
        Ok(())
    })
}

#[then(
    "the read is refused with `TooLarge`, no entry of that span reaches the client, and the transaction's next request is refused"
)]
fn past_room(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    let refused = answer.refusal()?;
    assert_eq!(refused.code, ErrorCode::TooLarge, "{refused:?}");
    assert_eq!(refused.detail, 524_288, "{refused:?}");
    assert!(answer.batches.is_empty());
    // The Bind stream's own refusal of the next request is the partition's: `ultragraphdb_storage_tests`' read kernel scenario.
    Ok(())
}
