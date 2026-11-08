/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A tightening's check at the frontend, the partitions played: what each reports, and when, a scenario decides.

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_common_frontend::DropStep;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_mapping::{Cardinality, Declaration};
use ultragraphdb_protocol_wire::{
    BodyView, Command, Declared, DeclaredKind, DeclaredPredicate, Describe, DescribeView,
    ErrorCode, Report, RequestTerm, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::Steps;
use crate::endpoint_stage::{CLUSTER, Client, Options, Stage, fresh, pause, view_frame};
use crate::endpoint_steps_requests::{TIMER_MS, declare, run};

const NICK: &str = "http://ex.org/nick";
const AGE: &str = "http://ex.org/age";
const FRIEND: &str = "http://ex.org/friend";
const SUBJECT: &str = "http://ex.org/a";

/// How long a Commit is watched for an answer it must not give, in milliseconds.
const UNANSWERED_MS: u64 = 80;

/// How long a test waits for what its frontend does on its own, polling.
const WAIT: Duration = Duration::from_secs(5);

/// A stage of `partitions` partitions waiting `check_ms` for each one's report.
fn checking(partitions: u32, check_ms: u64) -> Options {
    Options {
        partitions,
        check_ms,
        ..Options::default()
    }
}

/// A Write declaring `nick` single-valued.
fn single_nick() -> Write {
    declaring(DeclaredPredicate::new(NICK).with_single(true))
}

fn declaring(predicate: DeclaredPredicate) -> Write {
    Write::new(
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(predicate))],
    )
}

/// Commits `write` on a new transaction, unanswered: the transaction's stream.
async fn committing(client: &mut Client, write: &Write) -> TestResult<u64> {
    let transaction = client.begin(0).await?;
    client.on(transaction, write).await?;
    client.commit(transaction).await?;
    Ok(transaction)
}

/// The catalog version above `after`, once its epoch is assigned: its number.
async fn assigned(stage: &Stage, after: u64) -> TestResult<u64> {
    let due = compio::time::timeout(WAIT, async {
        loop {
            if let Ok(held) = stage.owner.catalog(0, 0).await
                && held.version() > after
                && held.epoch() != 0
            {
                return held.version();
            }
            pause(1).await;
        }
    })
    .await;
    due.map_err(|_| TestError::new(format!("no version above {after} was assigned its epoch")))
}

/// Sends the role's `report`, answered once the group holds it.
async fn report(stage: &Stage, report: &Report) -> TestResult {
    let mut storage = RoleClient::dial(
        &stage.role,
        CLUSTER,
        "storage",
        stage.cluster.secret().password(),
    )
    .await?;
    let answer = storage.request(report).await?;
    assert_eq!(answer.view()?.command(), Command::Report, "held as sent");
    Ok(())
}

/// The report that `partition`'s data refuses version `version`'s `nick`, `a` at fault.
fn refused(partition: u32, version: u64) -> TestResult<Report> {
    Ok(Report::refused(
        partition,
        0,
        version,
        NICK,
        RequestTerm::Iri(SUBJECT.to_string()),
    )?)
}

/// What Describe says of `predicate`: whether it declares it single-valued.
async fn single_in_describe(client: &mut Client, predicate: &str) -> TestResult<bool> {
    let describe = client.send(&Describe::request(0)).await?;
    let got = client.got(describe).await?;
    let view = view_frame(&got.bytes)?;
    let answer = DescribeView::from_frame(&view)?.to_body()?;
    Ok(answer
        .predicates()
        .iter()
        .any(|held| held.predicate() == predicate && held.single()))
}

/// Whether the group's latest version declares `predicate` single-valued, with its number.
async fn latest(stage: &Stage, predicate: &str) -> TestResult<(u64, bool)> {
    let held = stage
        .owner
        .catalog(0, 0)
        .await
        .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
    let contents = CatalogContents::decode(0, held.version(), held.bytes())?;
    let single = contents.declarations().iter().any(|declaration| {
        matches!(declaration, Declaration::Predicate(p)
            if p.predicate().as_str() == predicate && p.cardinality() == Cardinality::Single)
    });
    Ok((held.version(), single))
}

/// The version above `after` holding no `predicate` single-valued, once it is in effect at a close: what no Describe may report
/// otherwise, read until then.
async fn restored_in_effect(
    stage: &Stage,
    client: &mut Client,
    predicate: &str,
    after: u64,
) -> TestResult<u64> {
    let restoring = assigned(stage, after).await?;
    let (held, single) = latest(stage, predicate).await?;
    assert_eq!(held, restoring);
    assert!(!single, "the latest version restores the declaration");
    for _ in 0..50 {
        assert!(
            !single_in_describe(client, predicate).await?,
            "no Describe reports the withdrawn declaration"
        );
        pause(1).await;
    }
    Ok(restoring)
}

#[when(
    "`a` holds two objects of `nick` and a client commits a declaration of `nick` single-valued"
)]
fn contradicted(w: &mut Steps) -> TestResult {
    run(w, checking(2, 8_000), |stage| async move {
        let mut client = stage.ann().await?;
        let commit = committing(&mut client, &single_nick()).await?;
        let version = assigned(&stage, 0).await?;
        assert!(
            !single_in_describe(&mut client, NICK).await?,
            "during the check Describe reports the version before"
        );
        report(&stage, &Report::passed(1, 0, version)).await?;
        report(&stage, &refused(2, version)?).await?;
        let got = client.got(commit).await?;
        assert_eq!(got.code, Some(ErrorCode::Refused), "{}", got.message);
        assert_eq!(got.detail, 0);
        assert!(
            got.message.contains(NICK) && got.message.contains(SUBJECT),
            "{}",
            got.message
        );
        let restoring = restored_in_effect(&stage, &mut client, NICK, version).await?;
        assert_eq!(restoring, version + 1);
        Ok(())
    })
}

#[then(
    "the Commit is refused with code 7 naming `nick` and `a`, the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued"
)]
fn contradicted_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client commits a declaration of `nick` single-valued over two partitions holding `a` with one object of it, and one has reported its check passed"
)]
fn borne_out(w: &mut Steps) -> TestResult {
    run(w, checking(2, 8_000), |stage| async move {
        let mut client = stage.ann().await?;
        let commit = committing(&mut client, &single_nick()).await?;
        let version = assigned(&stage, 0).await?;
        report(&stage, &Report::passed(1, 0, version)).await?;
        client.silent(commit, UNANSWERED_MS).await?;
        assert!(
            !single_in_describe(&mut client, NICK).await?,
            "Describe reports the declaration before until the check ends"
        );
        report(&stage, &Report::passed(2, 0, version)).await?;
        let got = client.got(commit).await?;
        assert_eq!(got.command, Command::Commit, "{}", got.message);
        let held = stage
            .owner
            .catalog(0, version)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let view = view_frame(&got.bytes)?;
        let answer = ultragraphdb_protocol_wire::CommitView::from_frame(&view)?;
        assert_eq!(answer.commit_version(), held.epoch() << 22);
        for _ in 0..1_000 {
            if single_in_describe(&mut client, NICK).await? {
                return Ok(());
            }
            pause(1).await;
        }
        Err(TestError::new("Describe never reported the version"))
    })
}

#[then(
    "the Commit stays unanswered and Describe reports `nick` as before until the other reports, and then the Commit answers the first version of the effective epoch and Describe reports `nick` single-valued"
)]
fn borne_out_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client commits a declaration of `nick` single-valued, and a partition holding the tenant reports nothing within `catalog_check_ms`"
)]
fn silent(w: &mut Steps) -> TestResult {
    run(w, checking(2, u64::from(TIMER_MS)), |stage| async move {
        let mut client = stage.ann().await?;
        let commit = committing(&mut client, &single_nick()).await?;
        let version = assigned(&stage, 0).await?;
        report(&stage, &Report::passed(1, 0, version)).await?;
        let got = client.got(commit).await?;
        assert_eq!(got.code, Some(ErrorCode::TimedOut), "{}", got.message);
        assert_eq!(got.detail, 2, "the partition silent");
        assert!(
            got.message.contains("partition 2") && got.message.contains(&version.to_string()),
            "{}",
            got.message
        );
        let restoring = restored_in_effect(&stage, &mut client, NICK, version).await?;
        report(&stage, &Report::passed(2, 0, version)).await?;
        report(&stage, &refused(2, version)?).await?;
        assert_eq!(latest(&stage, NICK).await?, (restoring, false));
        assert!(!single_in_describe(&mut client, NICK).await?);
        Ok(())
    })
}

#[then(
    "the Commit is refused with code 24, `detail` that partition, naming it and the version, the metadata group holds a version restoring the previous declaration, and a report arriving later changes nothing"
)]
fn silent_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client commits a declaration of `nick` single-valued, and before any partition reports a second transaction begun on that version commits a declaration of `age`"
)]
fn built_on_checked(w: &mut Steps) -> TestResult {
    run(w, checking(1, 8_000), |stage| async move {
        let mut client = stage.ann().await?;
        let first = committing(&mut client, &single_nick()).await?;
        let version = assigned(&stage, 0).await?;
        let second = committing(&mut client, &declaring(DeclaredPredicate::new(AGE))).await?;
        let got = client.got(second).await?;
        assert_eq!(got.code, Some(ErrorCode::Conflict), "{}", got.message);
        assert_eq!(got.detail, 5, "of kind Catalog");
        assert!(
            got.message.contains(&version.to_string()),
            "{}",
            got.message
        );
        report(&stage, &Report::passed(1, 0, version)).await?;
        assert_eq!(client.got(first).await?.command, Command::Commit);
        declare(&mut client, DeclaredPredicate::new(AGE)).await
    })
}

#[then(
    "the second Commit is refused with code 2, kind 5, naming the version being checked, and once the first is answered a third transaction's declaration of `age` commits"
)]
fn built_on_checked_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("`friend` is declared an edge, and a client commits a declaration of `friend` a property")]
fn both_ways(w: &mut Steps) -> TestResult {
    run(w, checking(1, 8_000), |stage| async move {
        let mut client = stage.ann().await?;
        let edge = DeclaredPredicate::new(FRIEND).with_kind(DeclaredKind::Edge);
        let first = committing(&mut client, &declaring(edge)).await?;
        let version = assigned(&stage, 0).await?;
        report(&stage, &Report::passed(1, 0, version)).await?;
        assert_eq!(client.got(first).await?.command, Command::Commit);
        let property = DeclaredPredicate::new(FRIEND).with_kind(DeclaredKind::Property);
        let second = committing(&mut client, &declaring(property)).await?;
        let got = client.got(second).await?;
        assert_eq!(got.code, Some(ErrorCode::Refused), "{}", got.message);
        assert_eq!(got.detail, 0, "the index of the declaration that loosens");
        assert!(got.message.contains(FRIEND), "{}", got.message);
        assert_eq!(latest(&stage, FRIEND).await?.0, version, "unchanged");
        Ok(())
    })
}

#[then(
    "the Commit is refused with code 7, `detail` its declaration's index, naming `friend`, and the catalog is unchanged"
)]
fn both_ways_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client commits a declaration of `nick` single-valued, the frontend stops before any partition reports, and after its restart a partition reports that the data refuses it"
)]
fn restarted(w: &mut Steps) -> TestResult {
    let directory = fresh()?;
    let options = Options {
        directory: Some(directory.clone()),
        ..checking(1, 8_000)
    };
    run(w, options.clone(), |stage| async move {
        let mut client = stage.ann().await?;
        let _unanswered = committing(&mut client, &single_nick()).await?;
        let version = assigned(&stage, 0).await?;
        drop(client);
        stage.stop().await?;
        let again = Stage::start(options).await?;
        let mut asking = again.ann().await?;
        assert!(!single_in_describe(&mut asking, NICK).await?);
        report(&again, &refused(1, version)?).await?;
        let restoring = restored_in_effect(&again, &mut asking, NICK, version).await?;
        assert_eq!(restoring, version + 1);
        Ok(())
    })
}

#[then(
    "the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued"
)]
fn restarted_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client commits a declaration of `nick` and of `age` single-valued, a Drop of `nick` builds its version before any partition reports, and a partition reports that the data refuses the first"
)]
fn dropped_during(w: &mut Steps) -> TestResult {
    run(w, checking(1, 8_000), |stage| async move {
        let mut client = stage.ann().await?;
        let both = Write::new(
            TermTable::new(),
            [NICK, AGE]
                .map(|predicate| {
                    WriteChange::Declare(Declared::Predicate(
                        DeclaredPredicate::new(predicate).with_single(true),
                    ))
                })
                .to_vec(),
        );
        let commit = committing(&mut client, &both).await?;
        let version = assigned(&stage, 0).await?;
        stage
            .owner
            .drop_catalog(0, DropStep::Predicate(NICK.to_string()))
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let dropped = assigned(&stage, version).await?;
        report(&stage, &refused(1, version)?).await?;
        let got = client.got(commit).await?;
        assert_eq!(got.code, Some(ErrorCode::Refused), "{}", got.message);
        let restoring = assigned(&stage, dropped).await?;
        assert_eq!(restoring, dropped + 1);
        let held = stage
            .owner
            .catalog(0, restoring)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let contents = CatalogContents::decode(0, restoring, held.bytes())?;
        assert!(
            contents.declarations().is_empty(),
            "`nick` stays dropped, `age` goes back to no declaration: {:?}",
            contents.declarations()
        );
        Ok(())
    })
}

#[then(
    "the latest version holds neither `nick` nor `age` single-valued, and `nick` declared by no version after the Drop's"
)]
fn dropped_during_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
