/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Reverse reads and the keyspace a read takes by its bound positions.

use cucumber::{then, when};
use ultragraphdb_common_compute::prefix_end;
use ultragraphdb_core_keys::{Graph, Keyspace, key_prefix};
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::read_steps::{edge_with, reads};
use crate::scene::{KNOWS, Pattern, Scene, insert_of, iri, run};
use crate::{Steps, four};

/// `<a>` in partition 1 and `<c>` in partition 3 each `knows` `<b>`, homed in partition 2: `b`.
async fn referrers(scene: &Scene) -> TestResult<(String, Vec<String>)> {
    let (a, b, c) = (
        scene.iri_in(1, "a")?,
        scene.iri_in(2, "b")?,
        scene.iri_in(3, "c")?,
    );
    scene.insert(&a, KNOWS, &iri(&b)).await?;
    scene.insert(&c, KNOWS, &iri(&b)).await?;
    let order = scene.in_key_order(&[a, c]).await?;
    scene.info.log.clear();
    Ok((b, order))
}

/// The read of who `knows` `<b>` of `referrers`, at a snapshot past them, into `w`.
fn reverse_read(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (b, order) = referrers(&scene).await?;
        let read = scene.snapshot(&Pattern::of(None, Some(KNOWS), Some(iri(&b))))?;
        w.answers = vec![scene.answer(&read).await?];
        w.iris = order;
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[when(
    "`<a>` in partition 1 and `<c>` in partition 3 each hold `knows` to `<b>`, homed in partition 2, and a read binds its object to `<b>` and its predicate to `knows` at a closed epoch"
)]
fn gathered(w: &mut Steps) -> TestResult {
    reverse_read(w)
}

#[then("compute reads `R(b)` at partition 2, asks partitions 1 and 3, and returns `<a>` and `<c>`")]
fn gathers(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let log = &scene.info.log;
    let first = log.scans_of(2);
    assert!(
        first.iter().any(|scan| reads(scan, Keyspace::Referrers)),
        "{first:?}"
    );
    assert!(
        log.scans_of(1)
            .iter()
            .any(|scan| reads(scan, Keyspace::Reverse))
    );
    assert!(
        log.scans_of(3)
            .iter()
            .any(|scan| reads(scan, Keyspace::Reverse))
    );
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), w.iris);
    Ok(())
}

#[when(
    "`<b>` is homed in partition 2, `R(b)` names groups of partitions 1 and 3 only, over four partitions, and a reverse read of `<b>` runs"
)]
fn set_names_two(w: &mut Steps) -> TestResult {
    reverse_read(w)
}

#[then(
    "compute reads `R(b)` at partition 2, asks partitions 1 and 3 for their reverse entries, and sends nothing to partition 4"
)]
fn not_named(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    assert_eq!(scene.info.log.partitions(), vec![1, 2, 3]);
    w.answer()?.whole()
}

#[when(
    "`R(b)` names groups of partitions 1 and 3, partition 1 holds a matching row, partition 3 has not closed the read's epoch, and the transaction's deadline of 50 ms passes"
)]
fn lagging(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (b, _) = referrers(&scene).await?;
        let epoch = scene.snapshot_epoch() + 2;
        scene.info.clock.hold(3, epoch - 1);
        let read = scene
            .snapshot_at(&Pattern::of(None, Some(KNOWS), Some(iri(&b))), epoch)?
            .with_deadline_ms(50);
        let answer = scene.answer(&read).await;
        scene.info.clock.release(3);
        w.answers = vec![answer?];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "the read fails with a timeout naming partition 3, and returns no row of partition 1 without partition 3's"
)]
fn timed_out(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    let refused = answer.refusal()?;
    assert_eq!(refused.code, ErrorCode::TimedOut, "{refused:?}");
    assert_eq!(refused.detail, 3, "{refused:?}");
    assert!(answer.batches.is_empty(), "{:?}", answer.batches);
    Ok(())
}

#[when("a read binds its object to `http://ex.org/b` and its predicate to `http://ex.org/knows`")]
fn bound_object(w: &mut Steps) -> TestResult {
    reverse_read(w)
}

#[then(
    "it reads the reverse lists of `http://ex.org/b` under `http://ex.org/knows` in the partitions `R(b)` names, and returns each subject"
)]
fn reverse_lists(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let asked: Vec<u32> = scene
        .info
        .log
        .scans()
        .iter()
        .filter(|scan| reads(scan, Keyspace::Reverse))
        .map(|scan| scan.partition)
        .collect();
    assert!(asked.contains(&1) && asked.contains(&3), "{asked:?}");
    assert!(!asked.contains(&4), "{asked:?}");
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), w.iris);
    Ok(())
}

/// `<a> <p> <b>` in the default graph and in each of `graphs`: `a` and `b`.
async fn in_graphs(scene: &Scene, graphs: &[&str]) -> TestResult<(String, String)> {
    let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
    let object = iri(&b);
    scene
        .commit(1, &insert_of(None, &a, KNOWS, &object)?)
        .await?;
    for graph in graphs {
        scene
            .commit(1, &insert_of(Some(graph), &a, KNOWS, &object)?)
            .await?;
    }
    Ok((a, b))
}

#[when(
    "`<a> <p> <b>` is held in the default graph and in `<g1>`, and a read leaves the graph unbound"
)]
fn unbound_graph(w: &mut Steps) -> TestResult {
    const G1: &str = "http://ex.org/g1";
    let scene = Scene::new(four());
    run(async {
        let (a, b) = in_graphs(&scene, &[G1]).await?;
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None).any_graph())?;
        let of_one = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None).in_graph(G1))?;
        w.answers = vec![scene.answer(&read).await?, scene.answer(&of_one).await?];
        w.iris = vec![G1.to_string(), b];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then("it returns both statements, each with its graph")]
fn both_graphs(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    let [g1, b] = w.iris.as_slice() else {
        return Err(TestError::new("the terms"));
    };
    // The default graph sorts first, its 16 zero bytes below any system ID, and has no term.
    assert_eq!(answer.column(0), vec![String::new(), g1.clone()]);
    assert_eq!(answer.column(1), vec![b.clone(), b.clone()]);
    let of_one = w.answers.get(1).context("the read of one graph")?;
    of_one.whole()?;
    assert_eq!(of_one.column(0), vec![b.clone()]);
    Ok(())
}

#[when("`<a> <knows> <b>` is held without a statement ID and a read returns it")]
fn no_statement_id(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (a, b) = in_graphs(&scene, &[]).await?;
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?;
        w.answers = vec![scene.answer(&read).await?];
        w.iris = vec![b];
        Ok(())
    })?;
    Ok(())
}

#[then("the row names the statement by `<a>`, `<knows>`, `<b>` and its graph alone")]
fn named_by_quad(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.column(0), w.iris);
    assert_eq!(
        answer.rows().first().and_then(|row| row.get(1).cloned()),
        Some(None)
    );
    Ok(())
}

/// `<a> <knows> <b>` held, and a parallel edge of it: `a` and `b`.
async fn parallel(scene: &Scene) -> TestResult<(String, String)> {
    let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(2, "b")?);
    scene.insert(&a, KNOWS, &iri(&b)).await?;
    scene
        .commit(
            1,
            &edge_with(&a, KNOWS, &b, &[("http://ex.org/since", "2020")])?,
        )
        .await?;
    scene.info.log.clear();
    Ok((a, b))
}

#[when(
    "`<a> <knows> <b>` is held without a statement ID, a parallel edge of it is inserted, and a read returns both"
)]
fn parallel_edges(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (a, _) = parallel(&scene).await?;
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None))?;
        w.answers = vec![scene.answer(&read).await?];
        Ok(())
    })?;
    Ok(())
}

#[then(
    "two rows return, each carrying its own statement's reifier IRI, and neither carries a statement ID"
)]
fn own_reifiers(w: &mut Steps) -> TestResult {
    let answer = w.answer()?;
    answer.whole()?;
    let reifiers = answer.column(1);
    assert_eq!(reifiers.len(), 2);
    assert_ne!(reifiers.first(), reifiers.get(1));
    for reifier in &reifiers {
        assert!(reifier.contains("/.well-known/genid/"), "{reifier}");
    }
    let rows = answer.rows();
    let blank = rows.iter().all(|row| {
        row.get(1)
            .and_then(Clone::clone)
            .is_some_and(|cell| cell.blank)
    });
    assert!(blank, "{rows:?}");
    Ok(())
}

#[when(
    "`<a> <knows> <b>` is held as two parallel edges, and a read binds its object to `<b>` and its predicate to `knows`"
)]
fn reverse_parallel(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four());
    run(async {
        let (a, b) = parallel(&scene).await?;
        let read = scene.snapshot(&Pattern::of(None, Some(KNOWS), Some(iri(&b))))?;
        w.answers = vec![scene.answer(&read).await?];
        w.iris = vec![a];
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "two rows return for `<a>`, each with its statement's reifier from the reverse list's object record, and compute sends no Scan of a forward list"
)]
fn reverse_reifiers(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let scans = scene.info.log.scans();
    assert!(
        !scans.iter().any(|scan| reads(scan, Keyspace::Forward)),
        "{scans:?}"
    );
    let answer = w.answer()?;
    answer.whole()?;
    let a = w.iris.first().context("the subject")?;
    assert_eq!(answer.column(0), vec![a.clone(), a.clone()]);
    let reifiers = answer.column(1);
    assert_ne!(reifiers.first(), reifiers.get(1));
    assert!(reifiers.iter().all(|reifier| !reifier.is_empty()));
    Ok(())
}

#[when(
    "`<a> <p> <b>` is held in `<g1>` and `<g2>`, and a read of `<a> <p> ?o` leaves the graph unbound"
)]
fn walked_graphs(w: &mut Steps) -> TestResult {
    const G1: &str = "http://ex.org/g1";
    const G2: &str = "http://ex.org/g2";
    let scene = Scene::new(four());
    run(async {
        let (a, b) = (scene.iri_in(1, "a")?, scene.iri_in(1, "b")?);
        let object = iri(&b);
        scene
            .commit(1, &insert_of(Some(G1), &a, KNOWS, &object)?)
            .await?;
        scene
            .commit(1, &insert_of(Some(G2), &a, KNOWS, &object)?)
            .await?;
        scene.info.log.clear();
        let read = scene.snapshot(&Pattern::of(Some(&a), Some(KNOWS), None).any_graph())?;
        w.answers = vec![scene.answer(&read).await?];
        let (one, two) = (scene.id_in(1, G1).await?, scene.id_in(1, G2).await?);
        let mut graphs = [one, two];
        graphs.sort_unstable();
        w.graph_keys = graphs
            .iter()
            .map(|id| {
                prefix_end(&key_prefix(
                    Keyspace::Forward,
                    false,
                    scene.tenant,
                    Some(Graph::Named(*id)),
                    None,
                ))
            })
            .collect();
        Ok(())
    })?;
    w.scene = Some(scene);
    Ok(())
}

#[then(
    "compute finds each graph by a Scan of limit 1 from the keyspace's tenant and then from just past the graph found, cancels each after its first frame, and returns both statements"
)]
fn walk_scans(w: &mut Steps) -> TestResult {
    let scene = w.scene()?;
    let walks: Vec<Vec<u8>> = scene
        .info
        .log
        .scans()
        .iter()
        .filter(|scan| scan.limit == 1 && reads(scan, Keyspace::Forward))
        .filter_map(|scan| scan.ranges.first().map(|range| range.first.clone()))
        .collect();
    let tenant_start = key_prefix(Keyspace::Forward, false, scene.tenant, None, None);
    assert_eq!(walks.first(), Some(&tenant_start), "{walks:?}");
    assert_eq!(walks.get(1), w.graph_keys.first(), "{walks:?}");
    assert_eq!(walks.get(2), w.graph_keys.get(1), "{walks:?}");
    let answer = w.answer()?;
    answer.whole()?;
    assert_eq!(answer.rows().len(), 2);
    Ok(())
}
