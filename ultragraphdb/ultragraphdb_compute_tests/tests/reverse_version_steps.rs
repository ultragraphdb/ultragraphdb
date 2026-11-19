/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Reverse reads and the versions they read at: a later latest-state reverse read of another home, a reverse read at a named
//! version, and a resumed statement read past a property removed since.

use cucumber::{then, when};
use ultragraphdb_protocol_wire::{
    ComputeRead, ErrorCode, ReadLevel, StatementTarget, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::scene::{KNOWS, NAME, Pattern, Scene, edge_with, iri, run, string};
use crate::{Steps, four};

#[when(
    "a read at latest level bound to one partition reverses over an object another partition homes and nobody holds"
)]
fn later_reverse(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
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
    let scene = Scene::new(four()?);
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

#[when(
    "a read of a reifier at latest level with limit 2 is resumed, the last property it returned having been removed"
)]
fn removed_property(w: &mut Steps) -> TestResult {
    let scene = Scene::new(four()?);
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

/// The same read with `key` in its resume section.
pub(crate) fn resumed(read: &ComputeRead, key: Vec<u8>) -> ComputeRead {
    let again = ComputeRead::new(
        read.tenant(),
        read.read_only(),
        read.level(),
        read.terms().clone(),
        *read.pattern(),
        key,
    )
    .with_principal(read.principal())
    .with_limit(read.limit())
    .with_deadline_ms(read.deadline_ms())
    .with_client_limits(read.max_frame(), read.credit())
    .with_epoch(read.epoch())
    .with_partition(read.partition())
    .with_transaction(read.transaction());
    if read.named() {
        again.with_named_version(read.read_version())
    } else {
        again.with_read_version(read.read_version())
    }
}
