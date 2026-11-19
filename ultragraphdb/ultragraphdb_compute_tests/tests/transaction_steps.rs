/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read of a read-write transaction: a Scan naming it, seeing its writes and recording what its commit validates.

use cucumber::{then, when};
use ultragraphdb_core_ids::Version;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_protocol_wire::{Cancel, ErrorCode, ReadLevel, TermTable, Write, WriteChange};
use ultragraphdb_store_quads::term_key;
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::played_partition::Script;
use crate::scene::{
    KNOWS, NAME, Pattern, Scene, Transaction, compute_client, insert_of, iri, reads, run, string,
    until,
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

/// Writes something unrelated in `first`, then commits: its refusal's conflict kind, `None` for a commit that holds.
async fn commit_a_write(scene: &Scene, first: &mut Transaction) -> TestResult<Option<u64>> {
    first.write(&unrelated(scene)?).await?;
    match first.try_commit().await? {
        Ok(_) => Ok(None),
        Err(refused) => {
            assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
            Ok(Some(refused.detail))
        }
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
        let own = scene.begin(1, 1).await?;
        let read = own.read(&scene, &Pattern::of(None, Some(NAME), None), 0)?;
        w.answers = vec![scene.answer(&read).await?];
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
        let mut first = scene.begin(1, 1).await?;
        let log = &scene.info.log;
        log.clear();
        // The dictionary's Scan goes on; the Scan of `<a>`'s list, the one more span, waits at the tap for compute's Cancel of it.
        log.pause_until_next_frame(1);
        let read = first.read(&scene, &Pattern::of(Some(&a), Some(KNOWS), None), 0)?;
        let mut client = compute_client(scene.info).await?;
        client.send_only(&read).await?;
        let stream = client.last_stream();
        until("two Scans seen", || log.scans().len() >= 2).await?;
        assert_eq!(log.scans().len(), 2, "the list's Scan held");
        // The Cancel a Commit sends; compute's own Cancel of its Scan lets the Scan held go on, then follows it on the same connection.
        client.send_on(stream, &Cancel).await?;
        // The tap lets compute's Cancel go on once the partition has replied to the Scan held, so the Scan has run before it.
        until("a Cancel seen", || log.cancels() >= 1).await?;
        log.go_on();
        assert_eq!(log.cancels(), 1, "compute's Cancel of its Scan");
        scene
            .commit(1, &insert_of(None, &a, KNOWS, &iri(&b))?)
            .await?;
        w.kind = commit_a_write(&scene, &mut first).await?;
        let late = compio::time::timeout(SILENCE, client.next_frame()).await;
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
        let read = scene
            .read_of(&pattern, false, ReadLevel::Snapshot)?
            .with_partition(1)
            .with_transaction([9; 16])
            .with_read_version(Version::bound_of(ultragraphdb_core_ids::Epoch::new(9)?));
        w.answers = vec![scene.answer(&read).await?];
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
