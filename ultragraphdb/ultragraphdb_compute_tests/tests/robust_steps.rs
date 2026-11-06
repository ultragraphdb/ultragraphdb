/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a read holds through faults and load: a Scan stalled behind the merge, a corrupt stored value, a pin refused, a wide list's
//! names, and the connections concurrent reads share.

use std::rc::Rc;
use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_common_compute::{ComputeCore, LookupAt, ReadTimes, read_times_of};
use ultragraphdb_common_role::RoleCluster;
use ultragraphdb_common_service::{RoleIdentity, RoleStart};
use ultragraphdb_core_ids::{SystemId, Tenant, TypePrefix};
use ultragraphdb_core_keys::{CountDirection, Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    Credit, ErrorCode, ErrorReply, ObjectRecordsBuilder, ReadLevel, StatementTarget, TermTable,
    Write, WriteChange,
};
use ultragraphdb_store_quads::{id_key, term_key};
use ultragraphdb_tests::{CLUSTER, SECRET, holding, render_settings, scratch};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::played_partition::{PlayedTables, Script};
use crate::read_steps::{edge_with, reads, until};
use crate::resume_steps::{large_list, resumed};
use crate::scene::{
    FLOOR, KNOWS, NAME, Pattern, Scene, answer_on, compute_client, iri, next_batch, run, string,
};
use crate::{Steps, four, played, played_tables};

/// The first token a lookup names, the `term` tokenizer's.
const TERM: (u8, &[u8]) = (1, b"ann");

/// An ID of `sequence` in `scene`'s tenant's run.
fn id_of(scene: &Scene, sequence: u64) -> TestResult<SystemId> {
    let run = ultragraphdb_cluster_placement::TenantRun::of(Tenant::new(scene.tenant)).range();
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        ultragraphdb_core_ids::Bucket::new(run.start())?,
        ultragraphdb_core_ids::Sequence::new(sequence)?,
    ))
}

/// Has `tables` hold `iri` under `id`, by its text and its ID.
fn term(tables: &PlayedTables, scene: &Scene, iri: &str, id: SystemId) {
    tables.put(term_key(scene.tenant, iri), id.to_be_bytes().to_vec());
    tables.put(id_key(scene.tenant, id), iri.as_bytes().to_vec());
}

/// One object record of each of `ids`, a list's value.
fn list_of(ids: &[SystemId]) -> Vec<u8> {
    let mut records = ObjectRecordsBuilder::new();
    for id in ids {
        records.push(EncodedTerm::id(*id).as_bytes(), &[]);
    }
    records.value().to_vec()
}

/// A core of this process over the played partitions, reading by `times`.
fn core_over_played(times: ReadTimes) -> TestResult<Rc<ComputeCore>> {
    let info = played();
    let settings = holding(
        vec![
            (
                "observability.address".to_string(),
                format!("tcp://127.0.0.1:{}", info.base + 41),
            ),
            ("cluster.id".to_string(), CLUSTER.to_string()),
        ],
        SECRET,
    )?;
    let start = RoleStart::from_settings(
        render_settings(&settings).as_bytes(),
        RoleIdentity::new("ci", "compute", 1),
    )?;
    let cluster = RoleCluster::read(&start)?;
    let frontend = Address::parse(&format!("tcp://127.0.0.1:{}", info.base + 1))?;
    Ok(ComputeCore::new(cluster, frontend, times, "compute"))
}

/// A lookup's refusal as a test reads it.
fn refusal_text(reply: &ErrorReply) -> TestError {
    TestError::new(format!("{}: {}", reply.code().number(), reply.message()))
}

/// Partition 1 holds `<s1> <name>` three objects and `<s2> <name>` one: the read of `?s <name> ?o`, and how many rows it holds.
fn list_two_subjects(scene: &Scene) -> TestResult<(Vec<u8>, usize)> {
    let tables = played_tables(1)?;
    let name = id_of(scene, 500)?;
    term(tables, scene, NAME, name);
    let objects: Vec<SystemId> = (510..514)
        .map(|sequence| id_of(scene, sequence))
        .collect::<TestResult<_>>()?;
    for (at, object) in objects.iter().enumerate() {
        term(tables, scene, &format!("http://ex.org/o{at}"), *object);
    }
    let lists = [
        (id_of(scene, 501)?, objects.get(..3).context("three")?),
        (id_of(scene, 502)?, objects.get(3..).context("one")?),
    ];
    for (at, (subject, list)) in lists.iter().enumerate() {
        term(tables, scene, &format!("http://ex.org/s{at}"), *subject);
        let key = QuadKey::Forward {
            tenant: scene.tenant,
            graph: Graph::Default,
            predicate: name,
            subject: *subject,
        }
        .to_bytes();
        tables.put(key, list_of(list));
    }
    Ok((
        key_prefix(
            Keyspace::Forward,
            false,
            scene.tenant,
            Some(Graph::Default),
            Some(name),
        ),
        objects.len(),
    ))
}

#[when("a partition's Scan stalls behind the merge, once between two keys and once inside a list")]
fn stalls_behind(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let (first, rows) = list_two_subjects(&scene)?;
    let pattern = Pattern::of(None, Some(NAME), None);
    let tables = played_tables(1)?;
    run(async {
        let read = scene.snapshot_at(&pattern, 1)?;
        tables.script(first.clone(), Script::StallPastKey);
        let past_key = scene.answer(&read).await?;
        tables.script(first.clone(), Script::StallInList);
        let in_list = scene.answer(&read).await?;
        w.answers = vec![past_key, in_list];
        Ok(())
    })?;
    w.numbers = vec![rows as u64];
    Ok(())
}

#[then("the read reaches `LAST` with each row once, and the Scan went on from where it stopped")]
fn went_on(w: &mut Steps) -> TestResult {
    let rows = *w.numbers.first().context("the rows")? as usize;
    assert_eq!(w.answers.len(), 2);
    for answer in &w.answers {
        answer.whole()?;
        let mut held = answer.column(1);
        held.sort();
        held.dedup();
        assert_eq!(answer.rows().len(), rows, "{:?}", answer.column(1));
        assert_eq!(held.len(), rows, "{held:?}");
    }
    Ok(())
}

#[when("a partition's Scan stalls before giving a key")]
fn stalls_at_once(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let (first, _) = list_two_subjects(&scene)?;
    let pattern = Pattern::of(None, Some(NAME), None);
    let tables = played_tables(1)?;
    tables.script(first, Script::Refuse(ErrorCode::TimedOut, 1));
    run(async {
        let read = scene.snapshot_at(&pattern, 1)?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("the read is refused with code 24, `detail` that partition")]
fn stalled_read(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::TimedOut, "{refused:?}");
    assert_eq!(refused.detail, 1, "{refused:?}");
    Ok(())
}

#[when(
    "a statement's stored record does not read, and a client's connection then reads another list of that partition"
)]
fn corrupt_record(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let tables = played_tables(1)?;
    let (bad, good) = (scene.iri_in(1, "bad")?, scene.iri_in(1, "good")?);
    let (bad_id, good_id) = (id_of(&scene, 620)?, id_of(&scene, 621)?);
    term(tables, &scene, &bad, bad_id);
    term(tables, &scene, &good, good_id);
    let predicates_of = |subject| QuadKey::SubjectPredicates {
        tenant: scene.tenant,
        graph: Graph::Default,
        subject,
    };
    let mut naming_no_id = ObjectRecordsBuilder::new();
    let hash = Hash128::of(b"no ID");
    naming_no_id.push(&[&[0x21][..], hash.as_bytes()].concat(), &[]);
    tables.put(
        predicates_of(bad_id).to_bytes(),
        naming_no_id.value().to_vec(),
    );
    tables.put(predicates_of(good_id).to_bytes(), list_of(&[]));
    run(async {
        let mut client = compute_client(scene.info).await?;
        let read_of = |subject: &str| {
            scene.read_of(
                &Pattern::of(Some(subject), None, None),
                true,
                ReadLevel::Latest,
            )
        };
        let refused = answer_on(&mut client, &read_of(&bad)?).await?;
        let dialled = tables.connections();
        let answered = answer_on(&mut client, &read_of(&good)?).await?;
        w.numbers = vec![u64::from(tables.connections() - dialled)];
        w.answers = vec![refused, answered];
        Ok(())
    })
}

#[then(
    "the first read is refused with code 17, and the second is answered over the same connection to the partition"
)]
fn own_read_alone(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::Unavailable, "{refused:?}");
    w.answers.get(1).context("the second read")?.whole()?;
    assert_eq!(w.numbers.first(), Some(&0), "no connection dialled again");
    Ok(())
}

#[when(
    "a read at latest level bound to one partition reverses over an object another partition homes and nobody holds"
)]
fn later_reverse(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let object = scene.iri_in(2, "nobody")?;
        let read = scene
            .read_of(
                &Pattern::of(None, Some(KNOWS), Some(iri(&object))),
                true,
                ReadLevel::Latest,
            )?
            .with_partition(1);
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("the read is refused with code 25")]
fn level_refused(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::LevelRefused, "{refused:?}");
    Ok(())
}

#[when(
    "an edge's commit version in its partition lies below its member's version in the home's own order, and a reverse read names the edge's version"
)]
fn named_reverse(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    let clock = &scene.info.clock;
    run(async {
        let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(2, "b")?);
        let filler = scene.iri_in(2, "filler")?;
        // The predicates are registered at their homes first: a commit relying on a home's newer close carries that epoch as
        // its floor, and waits at a held partition for a close the hold withholds.
        scene.insert(&filler, NAME, &string("first")).await?;
        scene.insert(&a, KNOWS, &iri(&filler)).await?;
        let held = clock.latest();
        clock.hold(1, held);
        clock.hold(2, held);
        scene.closed_through(held, &[1, 2]).await?;
        let mut home_version = 0;
        for at in 0..16 {
            home_version = scene
                .insert_version(&filler, NAME, &string(&at.to_string()))
                .await?;
        }
        let edge_version = scene.insert_version(&a, KNOWS, &iri(&b)).await?;
        let read = scene
            .read_of(
                &Pattern::of(None, Some(KNOWS), Some(iri(&b))),
                true,
                ReadLevel::Snapshot,
            )?
            .with_named_version(edge_version);
        let answer = scene.answer(&read).await;
        clock.release(1);
        clock.release(2);
        assert!(
            edge_version < home_version,
            "the edge {edge_version} lies below what the home stamped first, {home_version}"
        );
        w.answers = vec![answer?];
        w.iris = vec![a];
        Ok(())
    })
}

#[then("the read returns the edge")]
fn returns_edge(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), w.iris);
    Ok(())
}

/// A Write deleting `predicate` `value` of the statement of `reifier`.
fn property_removal(reifier: &str, predicate: &str, value: &str) -> TestResult<Write> {
    let mut table = TermTable::new();
    let statement = StatementTarget::Reifier {
        graph: None,
        reifier: table.iri(reifier),
    };
    let predicate = table.iri(predicate);
    let value = table.intern(string(value))?;
    Ok(Write::new(
        table,
        vec![WriteChange::DeleteProperty {
            statement,
            predicate,
            value: Some(value),
        }],
    ))
}

#[when(
    "a read of a reifier at latest level with limit 2 is resumed, the last property it returned having been removed"
)]
fn removed_property(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
        let properties = [
            ("http://ex.org/p1", "1"),
            ("http://ex.org/p2", "2"),
            ("http://ex.org/p3", "3"),
        ];
        scene
            .commit(1, &edge_with(&a, KNOWS, &b, &properties)?)
            .await?;
        let of_edge = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?;
        let reifier = scene
            .answer(&of_edge)
            .await?
            .column(1)
            .first()
            .cloned()
            .context("its reifier")?;
        let read = scene
            .read_of(
                &Pattern::of(Some(&reifier), None, None),
                true,
                ReadLevel::Latest,
            )?
            .with_limit(2);
        let first = scene.answer(&read).await?;
        let predicate = first.column(0).get(1).cloned().context("a predicate")?;
        let value = first.column(1).get(1).cloned().context("a value")?;
        scene
            .commit(1, &property_removal(&reifier, &predicate, &value)?)
            .await?;
        let second = scene.answer(&resumed(&read, first.resume())).await?;
        w.answers = vec![first, second];
        Ok(())
    })
}

#[then("the resumed read returns the third property alone")]
fn third_alone(w: &mut Steps) -> TestResult {
    let [first, second] = w.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    first.whole()?;
    second.whole()?;
    let returned = first.column(0);
    assert_eq!(second.rows().len(), 1, "{:?}", second.column(0));
    let third = second.column(0);
    assert!(
        third.iter().all(|predicate| !returned.contains(predicate)),
        "{third:?} after {returned:?}"
    );
    Ok(())
}

#[when("a read with limit 2 reads a list of many objects held by hash")]
fn limit_names(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = large_list(&scene, 40, 200).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_limit(2);
        w.answers = vec![scene.answer(&read).await?];
        w.numbers = scene
            .info
            .log
            .scans_of(1)
            .iter()
            .map(|scan| scan.ranges.len() as u64)
            .collect();
        Ok(())
    })
}

#[then("it asks the partition's dictionary for the names of those rows alone")]
fn own_names(w: &mut Steps) -> TestResult {
    w.answer()?.whole()?;
    assert_eq!(w.answer()?.rows().len(), 2);
    let ranges: u64 = w.numbers.iter().sum();
    // The subject's and predicate's term, the list, and two rows' objects.
    assert!(ranges <= 6, "ranges asked: {:?}", w.numbers);
    Ok(())
}

#[when("a client of no further credit reads a list whose objects' texts come to many frames")]
fn no_more_credit(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let a = large_list(&scene, 64, 100_000).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_client_limits(FLOOR, 1);
        let mut client = compute_client(scene.info).await?;
        client.send_only(&read).await?;
        let _batch = next_batch(&mut client).await?;
        compio::time::sleep(Duration::from_millis(100)).await;
        w.numbers = vec![scene.info.log.versions().len() as u64];
        Ok(())
    })
}

#[then("the partition sent no more than a few frames of them")]
fn few_frames(w: &mut Steps) -> TestResult {
    let frames = *w.numbers.first().context("the frames")?;
    // The terms', the list's and those of about a Result frame's names, at a turn's 256 KiB: 6,400,000 bytes of text would take 25.
    assert!(frames <= 12, "{frames} frames");
    Ok(())
}

/// The value index of `<name>` under `ann` in every played partition, listing `subject`, each with `<name>` and `subject` named.
fn played_index(scene: &Scene, subject: SystemId) -> TestResult<Vec<u8>> {
    let predicate = id_of(scene, 900)?;
    let key = QuadKey::ValueIndex {
        tenant: scene.tenant,
        graph: Graph::Default,
        predicate,
        tokenizer: TERM.0,
        token: TERM.1.to_vec(),
    }
    .to_bytes();
    for partition in 1..=3 {
        let tables = played_tables(partition)?;
        term(tables, scene, NAME, predicate);
        term(tables, scene, "http://ex.org/s", subject);
        tables.put(key.clone(), list_of(&[subject]));
    }
    Ok(key)
}

/// How many connections each played partition has taken.
fn connections() -> TestResult<Vec<u32>> {
    (1..=3)
        .map(|partition| Ok(played_tables(partition)?.connections()))
        .collect()
}

#[when("two lookups start together on a core that holds no connection")]
fn together(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let _key = played_index(&scene, id_of(&scene, 901)?)?;
    let before = connections()?;
    run(async {
        let core = core_over_played(read_times_of(None, None))?;
        let at = LookupAt::new(1, 1_000);
        let (one, two) = futures::future::join(
            core.value_index(scene.tenant, at, NAME, TERM),
            core.value_index(scene.tenant, at, NAME, TERM),
        )
        .await;
        drop(one.map_err(|reply| refusal_text(&reply))?);
        drop(two.map_err(|reply| refusal_text(&reply))?);
        Ok(())
    })?;
    w.numbers = connections()?
        .iter()
        .zip(&before)
        .map(|(now, then)| u64::from(now - then))
        .collect();
    Ok(())
}

#[then("each partition took one connection")]
fn one_connection(w: &mut Steps) {
    assert_eq!(w.numbers, vec![1, 1, 1]);
}

/// More objects of a list than an Entries frame holds, held by hash.
const ROWS_PAST_A_FRAME: u32 = 12_000;

/// How long the frame past its credit has to reach compute, before the client lets the read go on.
const PAST_CREDIT_ARRIVES: Duration = Duration::from_millis(20);

#[when("a partition sends a Scan's stream an Entries frame with the stream's credit spent")]
fn past_credit(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        // Held by hash, so one frame of the list holds far more rows than a Result frame's worth of their texts.
        let a = large_list(&scene, ROWS_PAST_A_FRAME, 1_000).await?;
        let read = scene
            .snapshot(&Pattern::of(Some(&a), Some(NAME), None))?
            .with_client_limits(FLOOR, 1);
        let mut client = compute_client(scene.info).await?;
        // The partition goes on past compute's grants, while compute drains the list's first frame at the client's credit.
        scene.info.log.overgrant_lists(64);
        client.send_only(&read).await?;
        let stream = client.last_stream();
        let mut answer = crate::scene::ReadAnswer::default();
        loop {
            match crate::scene::next_of(&mut client).await? {
                Ok(batch) => {
                    let last = batch.last;
                    answer.batches.push(batch);
                    if last {
                        break;
                    }
                }
                Err(refused) => {
                    answer.refused = Some(refused);
                    break;
                }
            }
            compio::time::sleep(PAST_CREDIT_ARRIVES).await;
            client.send_on(stream, &Credit::new(1)).await?;
        }
        let scans = scene.info.log.scans();
        let list = scans
            .iter()
            .find(|scan| reads(scan, Keyspace::Forward))
            .context("the list's Scan")?;
        let connection = list.connection;
        w.numbers = vec![list.stream];
        until("the role connection's end", || {
            scene.info.log.has_ended(connection)
        })
        .await?;
        let after = scene.iri_in(1, "after")?;
        let again = scene.snapshot(&Pattern::of(Some(&after), Some(NAME), None))?;
        w.answers = vec![answer, scene.answer(&again).await?];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then("compute refuses it as `PastCredit`, naming the stream, and closes the role connection")]
fn past_credit_closes(w: &mut Steps) -> TestResult {
    let (answer, after) = (
        w.answers.first().context("the read")?,
        w.answers.get(1).context("the next read")?,
    );
    assert_eq!(answer.refusal()?.code, ErrorCode::Unavailable);
    after.whole()?;
    let stream = *w.numbers.first().context("the list's stream")?;
    let said = ProtocolError::past_credit(stream).to_string();
    let base = w.scene()?.info.base;
    let log = std::fs::read_to_string(scratch(&format!("compute-{base}.log")))?;
    assert!(log.contains(&said), "`{said}` in compute's log: {log}");
    Ok(())
}

/// The `scan_stall_ms` of the lookup whose pin is refused.
const PIN_STALL_MS: u64 = 120;

/// How long the slow partition takes to answer that lookup's list.
const PIN_DELAY: Duration = Duration::from_millis(160);

#[when("a lookup outlasts its retention over three partitions, and one partition refuses its pin")]
fn pin_refused(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let key = played_index(&scene, id_of(&scene, 902)?)?;
    let pin = key_prefix(Keyspace::Referrers, false, scene.tenant, None, None);
    played_tables(1)?.script(pin, Script::Refuse(ErrorCode::TooOld, 0));
    // Slow within what compute waits for a partition, twice the stall, and long past the retention.
    played_tables(3)?.script(key, Script::After(PIN_DELAY));
    let credits = played_tables(2)?.credits();
    run(async {
        let core = core_over_played(read_times_of(Some(PIN_STALL_MS), Some(4)))?;
        let found = core
            .value_index(scene.tenant, LookupAt::new(1, 1_000), NAME, TERM)
            .await;
        drop(found.map_err(|reply| refusal_text(&reply))?);
        Ok(())
    })?;
    w.numbers = vec![u64::from(played_tables(2)?.credits() - credits)];
    Ok(())
}

#[then("the other partitions' pins go on taking frames")]
fn pins_go_on(w: &mut Steps) -> TestResult {
    let grants = *w.numbers.first().context("the grants")?;
    // A pin takes a frame each third of the stall; half of those within the delay shows the pins going on.
    let every = u128::from(PIN_STALL_MS / 3);
    let expected = u64::try_from(PIN_DELAY.as_millis() / every / 2).unwrap_or(u64::MAX);
    assert!(grants >= expected, "{grants} grants, {expected} expected");
    Ok(())
}

#[when(
    "`friend` lists many objects the dictionary does not name beside `<b>`, whose reverse count is 3, and compute's API looks up the objects of reverse count 3"
)]
fn counted_named(w: &mut Steps) -> TestResult {
    const FRIEND: &str = "http://ex.org/friend";
    const B: &str = "http://ex.org/b";
    let scene = Scene::new(played());
    let (friend, b) = (id_of(&scene, 800)?, id_of(&scene, 801)?);
    let unnamed: Vec<SystemId> = (810..860)
        .map(|sequence| id_of(&scene, sequence))
        .collect::<TestResult<_>>()?;
    for (partition, count) in [(1u32, 2u32), (2, 1)] {
        let tables = played_tables(partition)?;
        term(tables, &scene, FRIEND, friend);
        term(tables, &scene, B, b);
        let listed: Vec<SystemId> = if partition == 1 {
            unnamed.iter().copied().chain([b]).collect()
        } else {
            vec![b]
        };
        tables.put(
            QuadKey::CountIndex {
                tenant: scene.tenant,
                graph: Graph::Default,
                predicate: friend,
                direction: CountDirection::Reverse,
                count,
            }
            .to_bytes(),
            list_of(&listed),
        );
    }
    let asked: Vec<u32> = (1..=3)
        .map(|partition| Ok(played_tables(partition)?.asked()))
        .collect::<TestResult<_>>()?;
    w.texts = run(async {
        let core = core_over_played(read_times_of(None, None))?;
        core.reverse_counted(scene.tenant, LookupAt::new(1, 1_000), FRIEND, 3)
            .await
            .map_err(|reply| refusal_text(&reply))
    })?;
    w.iris = vec![B.to_string()];
    w.numbers = (1..=3u32)
        .zip(&asked)
        .map(|(partition, before)| Ok(u64::from(played_tables(partition)?.asked() - before)))
        .collect::<TestResult<_>>()?;
    Ok(())
}

#[then("the lookup returns `<b>` alone, in a few Scans of each partition")]
fn counted_alone(w: &mut Steps) {
    assert_eq!(w.texts, w.iris);
    assert!(w.numbers.iter().all(|asked| *asked <= 3), "{:?}", w.numbers);
}

/// Compute's API looks up `<name>` under `ann` over the played partitions: its refusal.
fn lookup_refusal(scene: &Scene) -> TestResult<ErrorReply> {
    run(async {
        let core = core_over_played(read_times_of(None, None))?;
        core.value_index(scene.tenant, LookupAt::new(1, 1_000), NAME, TERM)
            .await
            .err()
            .context("a refusal")
    })
}

#[when(
    "an index list names a system ID its partition's dictionary answers no entry for, and compute's API looks up the value"
)]
fn lookup_dangling(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let subject = id_of(&scene, 903)?;
    let _key = played_index(&scene, id_of(&scene, 904)?)?;
    let predicate = id_of(&scene, 900)?;
    played_tables(1)?.put(
        QuadKey::ValueIndex {
            tenant: scene.tenant,
            graph: Graph::Default,
            predicate,
            tokenizer: TERM.0,
            token: TERM.1.to_vec(),
        }
        .to_bytes(),
        list_of(&[subject]),
    );
    w.keys = vec![id_key(scene.tenant, subject)];
    let refused = lookup_refusal(&scene)?;
    w.numbers = vec![u64::from(refused.code().number())];
    w.texts = vec![refused.message().to_string()];
    Ok(())
}

#[then("the lookup is refused with code 17, the message naming the dictionary key")]
fn names_key(w: &mut Steps) -> TestResult {
    named_refusal(w)
}

#[when(
    "a partition's entry of the looked-up predicate does not decode as a system ID, and compute's API looks up the value"
)]
fn lookup_corrupt(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let _key = played_index(&scene, id_of(&scene, 905)?)?;
    let key = term_key(scene.tenant, NAME);
    played_tables(2)?.put(key.clone(), vec![0xFF; 3]);
    w.keys = vec![key];
    let refused = lookup_refusal(&scene)?;
    w.numbers = vec![u64::from(refused.code().number())];
    w.texts = vec![refused.message().to_string()];
    Ok(())
}

#[then("the lookup is refused with code 17, the message naming the entry's key")]
fn names_entry(w: &mut Steps) -> TestResult {
    named_refusal(w)
}

/// Fails unless the lookup's refusal is code 17, naming the scenario's key.
fn named_refusal(w: &Steps) -> TestResult {
    assert_eq!(
        w.numbers.first().copied(),
        Some(u64::from(ErrorCode::Unavailable.number()))
    );
    let key = w.keys.first().context("the key")?;
    let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    let message = w.texts.first().context("the message")?;
    assert!(message.contains(&hex), "{message}");
    Ok(())
}
