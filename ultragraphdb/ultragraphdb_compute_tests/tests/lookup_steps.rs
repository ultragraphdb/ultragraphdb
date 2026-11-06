/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What needs a partition played by the test: compute's lookups for the query core, until a catalog version reaches storage to
//! declare an index; a dangling ID; a refusal or a frame a real partition cannot be made to give.

use std::rc::Rc;

use cucumber::{then, when};
use ultragraphdb_common_compute::{ComputeCore, LookupAt, read_times_of};
use ultragraphdb_common_role::RoleCluster;
use ultragraphdb_common_service::{RoleIdentity, RoleStart};
use ultragraphdb_core_ids::{SystemId, Tenant, Version};
use ultragraphdb_core_keys::{CountDirection, Graph, QuadKey};
use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{ErrorCode, ObjectRecordsBuilder, ReadLevel};
use ultragraphdb_store_quads::{id_key, literal_key, term_key};
use ultragraphdb_tests::{CLUSTER, SECRET, holding, render_settings};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::played_partition::{PlayedTables, Script};
use crate::scene::{FLOOR, KNOWS, NAME, Pattern, Scene, run};
use crate::{Steps, played, played_tables};

/// The tag of an `xsd:string` held by hash (`PROTOCOL.md` §10.1).
const HASHED_STRING: u8 = 0x21;

/// The first token a lookup names, the `term` tokenizer's.
const TERM: (u8, &[u8]) = (1, b"ann");

/// An ID of `sequence`, in `partition`'s run of `scene`'s tenant.
fn id_of(scene: &Scene, sequence: u64) -> TestResult<SystemId> {
    let run = ultragraphdb_cluster_placement::TenantRun::of(Tenant::new(scene.tenant)).range();
    let bucket = ultragraphdb_core_ids::Bucket::new(run.start())?;
    Ok(SystemId::new(
        ultragraphdb_core_ids::TypePrefix::RESOURCE,
        bucket,
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

/// A core of this process over the played partitions, as a query core holds one.
fn core_over_played() -> TestResult<Rc<ComputeCore>> {
    let info = played();
    let settings = holding(
        vec![
            (
                "observability.address".to_string(),
                format!("tcp://127.0.0.1:{}", info.base + 40),
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
    Ok(ComputeCore::new(
        cluster,
        frontend,
        read_times_of(None, None),
        "compute",
    ))
}

#[when(
    "partitions 1, 2 and 3 each hold a subject whose `name` the `term` index lists under `ann`, and compute's API looks up `ann`"
)]
fn value_index(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let predicate = id_of(&scene, 900)?;
    let mut expected = Vec::new();
    for partition in 1..=3u32 {
        let tables = played_tables(partition)?;
        let subject = id_of(&scene, u64::from(4 - partition))?;
        let iri = format!("http://ex.org/s{partition}");
        term(tables, &scene, NAME, predicate);
        term(tables, &scene, &iri, subject);
        tables.put(
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
        expected.push((subject, iri));
    }
    expected.sort();
    let asked_before = asked_of(&[1, 2, 3])?;
    w.texts = run(async {
        let core = core_over_played()?;
        let at = LookupAt::new(1, 1_000);
        core.value_index(scene.tenant, at, NAME, TERM)
            .await
            .map_err(|reply| TestError::new(reply.message().to_string()))
    })?;
    w.numbers = (0..3)
        .map(|at| {
            let after = asked_of(&[at + 1])?;
            let before = *asked_before.get(at as usize).context("a count")?;
            Ok(after.first().map_or(0, |now| u64::from(*now - before)))
        })
        .collect::<TestResult<_>>()?;
    w.iris = expected.into_iter().map(|(_, iri)| iri).collect();
    Ok(())
}

fn asked_of(partitions: &[u32]) -> TestResult<Vec<u32>> {
    partitions
        .iter()
        .map(|partition| Ok(played_tables(*partition)?.asked()))
        .collect()
}

#[then(
    "compute asks the three partitions, and the API returns the three subjects merged in key order"
)]
fn three_subjects(w: &mut Steps) {
    assert!(w.numbers.iter().all(|asked| *asked > 0), "{:?}", w.numbers);
    assert_eq!(w.texts, w.iris);
}

#[when(
    "`friend` has an exact count index, two subjects in partition 1 and one in partition 2 hold `friend` to `<b>`, and compute's API looks up the objects of reverse count 3"
)]
fn reverse_count(w: &mut Steps) -> TestResult {
    const FRIEND: &str = "http://ex.org/friend";
    const B: &str = "http://ex.org/b";
    let scene = Scene::new(played());
    let (friend, b) = (id_of(&scene, 800)?, id_of(&scene, 801)?);
    for (partition, count) in [(1u32, 2u32), (2, 1)] {
        let tables = played_tables(partition)?;
        term(tables, &scene, FRIEND, friend);
        term(tables, &scene, B, b);
        tables.put(
            QuadKey::CountIndex {
                tenant: scene.tenant,
                graph: Graph::Default,
                predicate: friend,
                direction: CountDirection::Reverse,
                count,
            }
            .to_bytes(),
            list_of(&[b]),
        );
    }
    w.texts = run(async {
        let core = core_over_played()?;
        let at = LookupAt::new(1, 1_000);
        core.reverse_counted(scene.tenant, at, FRIEND, 3)
            .await
            .map_err(|reply| TestError::new(reply.message().to_string()))
    })?;
    w.iris = vec![B.to_string()];
    Ok(())
}

#[then(
    "partition 1 lists `<b>` under reverse count 2 and partition 2 under 1, and the lookup returns `<b>`"
)]
fn counted(w: &mut Steps) {
    assert_eq!(w.texts, w.iris);
}

/// A subject of partition 1 of `scene`'s tenant `knows` an ID the partition's dictionary lacks: the subject's IRI and the list's
/// key.
fn dangling_list(scene: &Scene) -> TestResult<(String, Vec<u8>)> {
    let subject = scene.iri_in(1, "a")?;
    let (subject_id, knows, dangling) =
        (id_of(scene, 700)?, id_of(scene, 701)?, id_of(scene, 702)?);
    let tables = played_tables(1)?;
    term(tables, scene, &subject, subject_id);
    term(tables, scene, KNOWS, knows);
    let key = QuadKey::Forward {
        tenant: scene.tenant,
        graph: Graph::Default,
        predicate: knows,
        subject: subject_id,
    }
    .to_bytes();
    tables.put(key.clone(), list_of(&[dangling]));
    Ok((subject, key))
}

#[when(
    "a list names a system ID its partition's dictionary answers no entry for, and a client's read through compute reaches it"
)]
fn dangling(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let (subject, key) = dangling_list(&scene)?;
    run(async {
        let pattern = Pattern::of(Some(&subject), Some(KNOWS), None);
        let read = scene.read_of(&pattern, true, ReadLevel::Latest)?;
        let mut client = crate::scene::compute_client(scene.info).await?;
        w.answers = vec![crate::scene::answer_on(&mut client, &read).await?];
        // The connection stays open: another read on it is answered.
        let next = crate::scene::answer_on(&mut client, &read).await?;
        w.answers.push(next);
        Ok(())
    })?;
    w.keys = vec![key];
    Ok(())
}

#[then(
    "the read is refused with code 17, the message naming the list's key, and the client's connection stays open"
)]
fn names_list(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::Unavailable, "{refused:?}");
    let key = w.keys.first().context("the key")?;
    let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    assert!(refused.message.contains(&hex), "{refused:?}");
    let next = w.answers.get(1).context("the next read")?;
    assert!(
        next.refused.is_some(),
        "the same read refused again, on the open connection"
    );
    Ok(())
}

#[when("a transaction's reads leave no room for a span")]
fn no_room(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
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

/// Rows a client of the floor's frame takes a batch each for.
const LARGE_LEXICAL: usize = 300_000;

/// The list's objects: the partition stops before the last, and a batch leaves once the row after it does not fit beside it, so
/// three batches leave before the refusal.
const RESTARTED_ROWS: u32 = 5;

#[when(
    "a read with no limit is refused with code 24 after three batches, and its caller runs it again"
)]
fn restarts(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let subject = scene.iri_in(1, "a")?;
    let (subject_id, name) = (id_of(&scene, 600)?, id_of(&scene, 601)?);
    let tables = played_tables(1)?;
    term(tables, &scene, &subject, subject_id);
    term(tables, &scene, NAME, name);
    let mut records = ObjectRecordsBuilder::new();
    for at in 0..RESTARTED_ROWS {
        let text = at.to_string().repeat(LARGE_LEXICAL);
        let hash = Hash128::of(text.as_bytes());
        tables.put(literal_key(scene.tenant, &hash), text.into_bytes());
        records.push(&[&[HASHED_STRING][..], hash.as_bytes()].concat(), &[]);
    }
    let key = QuadKey::Forward {
        tenant: scene.tenant,
        graph: Graph::Default,
        predicate: name,
        subject: subject_id,
    }
    .to_bytes();
    tables.put(key.clone(), records.value().to_vec());
    tables.script(key.clone(), Script::ThenRefuse(ErrorCode::TimedOut, 1));
    let read = scene
        .read_of(
            &Pattern::of(Some(&subject), Some(NAME), None),
            true,
            ReadLevel::Latest,
        )?
        .with_client_limits(FLOOR, 16);
    run(async {
        let stalled = scene.answer(&read).await?;
        tables.unscript(&key);
        let again = scene.answer(&read).await?;
        w.answers = vec![stalled, again];
        Ok(())
    })
}

#[then("no batch held a resume key, and the second run returns from the first row")]
fn second_run(w: &mut Steps) -> TestResult {
    let [stalled, again] = w.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    let refused = stalled.refusal()?;
    assert_eq!(refused.code, ErrorCode::TimedOut, "{refused:?}");
    assert_eq!(stalled.batches.len(), 3);
    assert!(stalled.batches.iter().all(|batch| batch.resume.is_empty()));
    again.whole()?;
    assert_eq!(again.rows().len(), RESTARTED_ROWS as usize);
    assert_eq!(again.column(0).first(), stalled.column(0).first());
    Ok(())
}

#[when("a partition takes a Scan and sends nothing more, its connection open")]
fn hung_partition(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let subject = scene.iri_in(1, "hung")?;
    let tables = played_tables(1)?;
    let dialled = tables.connections();
    tables.script(term_key(scene.tenant, &subject), Script::Silent);
    run(async {
        let pattern = Pattern::of(Some(&subject), Some(KNOWS), None);
        let read = scene.read_of(&pattern, true, ReadLevel::Latest)?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })?;
    // The next read dials the partition again, over a connection of its own.
    let other = scene.iri_in(1, "after")?;
    run(async {
        let pattern = Pattern::of(Some(&other), Some(KNOWS), None);
        let read = scene.read_of(&pattern, true, ReadLevel::Latest)?;
        w.answers.push(scene.answer(&read).await?);
        Ok(())
    })?;
    w.numbers = vec![u64::from(tables.connections() > dialled)];
    Ok(())
}

#[then(
    "compute answers the read with code 17 naming the partition, within twice its `scan_stall_ms`, and dials it again"
)]
fn hung_refused(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::Unavailable, "{refused:?}");
    assert!(refused.message.contains("partition 1"), "{refused:?}");
    // A Scan at the latest state waits on no epoch, so its bound is the link's alone.
    let stall: u64 = crate::PLAYED_SCAN_STALL_MS.parse()?;
    let bound = format!("{:?}", std::time::Duration::from_millis(stall * 2));
    assert!(refused.message.contains(&bound), "{refused:?}");
    assert_eq!(w.numbers, vec![1], "dialled again");
    w.answers.get(1).context("the next read")?.whole()?;
    Ok(())
}

#[when("a partition refuses a Scan with Error 1 of `detail` 1")]
fn relayed_detail(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let subject = scene.iri_in(1, "a")?;
    played_tables(1)?.script(
        term_key(scene.tenant, &subject),
        Script::Refuse(ErrorCode::Protocol, 1),
    );
    run(async {
        let pattern = Pattern::of(Some(&subject), Some(KNOWS), None);
        let read = scene.read_of(&pattern, true, ReadLevel::Latest)?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })
}

#[then("compute relays the Error with its `detail`")]
fn relays(w: &mut Steps) -> TestResult {
    let refused = w.answer()?.refusal()?;
    assert_eq!(refused.code, ErrorCode::Protocol, "{refused:?}");
    assert_eq!(refused.detail, 1, "{refused:?}");
    Ok(())
}

#[when(
    "a partition's answer holds a frame compute refuses, while another read waits on that connection"
)]
fn refused_frame(w: &mut Steps) -> TestResult {
    let scene = Scene::new(played());
    let subject = scene.iri_in(1, "broken")?;
    played_tables(1)?.script(term_key(scene.tenant, &subject), Script::BreakOrder);
    run(async {
        let pattern = Pattern::of(Some(&subject), Some(KNOWS), None);
        let read = scene.read_of(&pattern, true, ReadLevel::Latest)?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then("compute closes the connection, answers the read with code 17 and sends it no Scan again")]
fn closes(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    let refused = answer.refusal()?;
    assert_eq!(refused.code, ErrorCode::Unavailable, "{refused:?}");
    assert!(answer.batches.is_empty());
    assert!(refused.message.contains("refused"), "{refused:?}");
    Ok(())
}
