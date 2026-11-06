/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A tightening through the real frontend and storage: a partition reads its own data, and the client hears what it found.

use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, ClientErrorEnum, Committed};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, CatalogView, Declared, DeclaredKind, DeclaredPredicate, ErrorCode,
    RequestTerm, TermTable, Write, WriteChange,
};
use ultragraphdb_tests::{CLUSTER, SECRET};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{Pattern, TENANT, iri, read_whole, refused_with, string, write_of};
use crate::cluster::{
    Signal, Spec, client, iri_of_partition, placement_ranges, start_own, start_own_as,
};
use crate::shells::{TestClient, pause, run};

/// What the frontend logs of each report.
const REPORTED: &str = "a partition reported on a catalog version";

/// How long a step polls for what a role does on its own.
const POLLS: u32 = 500;
const POLL: Duration = Duration::from_millis(10);

/// What this file's steps saw.
#[derive(Debug, Default)]
pub(crate) struct Seen {
    /// The Commit's refusal: its code, detail and message.
    refusal: Option<(ErrorCode, u64, String)>,
    /// The Commit's version.
    answered: u64,
    /// The latest catalog version of the tenant before the tightening's and after it.
    versions: (u64, u64),
    /// What Describe said of the predicate: declared single-valued, after the scene.
    single: bool,
    /// Each report the frontend logged by the Commit's answer.
    reports: Vec<String>,
    /// A write's refusal by the catalog, and what the subject holds after it.
    objects: usize,
}

fn declaration(predicate: DeclaredPredicate) -> Write {
    Write::new(
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(predicate))],
    )
}

/// A client committing `write` on a new transaction.
async fn declared(client: &mut TestClient, write: &Write) -> Result<Committed, ClientError> {
    let transaction = client.begin(TENANT, false).await?;
    client.write(&transaction, write).await?;
    client.commit(transaction, None).await
}

/// The latest catalog version of the tenant, 0 for none, as a partition asks for it, on a runtime of its own: a role client is
/// compio's, whichever shell a run's client takes.
fn latest_version(frontend: &str) -> TestResult<u64> {
    let address = Address::parse(frontend)?;
    Runtime::new()?.block_on(async {
        let mut role = RoleClient::dial(&address, CLUSTER, "storage", SECRET).await?;
        let Ok(reply) = role.request(&Catalog::request(TENANT, 0)).await else {
            return Ok(0);
        };
        let view = reply.view()?;
        Ok(CatalogView::from_frame(&view)?.version())
    })
}

/// Whether Describe declares `predicate` single-valued.
async fn single(client: &mut TestClient, predicate: &str) -> TestResult<bool> {
    let described = client.describe(TENANT).await?;
    Ok(described
        .predicates()
        .iter()
        .any(|held| held.predicate() == predicate && held.single()))
}

/// Commits a declaration of `nick` single-valued over two objects of `a`, into `w`.
#[when(
    "`a` holds two objects of `nick` and a client commits a declaration of `nick` single-valued"
)]
fn contradicted(w: &mut Steps) -> TestResult {
    let (a, nick) = (w.iri("a"), w.iri("nick"));
    let cluster = start_own(w.carrier, 1)?;
    let before = latest_version(&cluster.frontend)?;
    let mut seen = run(async {
        let mut client = client(&cluster.client).await?;
        for value in ["x", "y"] {
            let write = write_of(&a, &nick, &string(value))?;
            let transaction = client.begin(TENANT, false).await?;
            client.write(&transaction, &write).await?;
            let _committed = client.commit(transaction, None).await?;
        }
        let answer = declared(
            &mut client,
            &declaration(DeclaredPredicate::new(&nick).with_single(true)),
        )
        .await;
        let (detail, message) = refused_with(answer, ErrorCode::Refused)?;
        Ok(Seen {
            refusal: Some((ErrorCode::Refused, detail, message)),
            single: single(&mut client, &nick).await?,
            ..Seen::default()
        })
    })?;
    seen.versions = (before, latest_version(&cluster.frontend)?);
    w.tightening = seen;
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "the Commit is refused with code 7 naming `nick` and `a`, the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued"
)]
fn contradicted_refused(w: &mut Steps) -> TestResult {
    let (a, nick) = (w.iri("a"), w.iri("nick"));
    let seen = &w.tightening;
    let (code, _, message) = seen.refusal.as_ref().context("the refusal")?;
    assert_eq!(*code, ErrorCode::Refused);
    assert!(message.contains(&a) && message.contains(&nick), "{message}");
    assert_eq!(
        seen.versions.1,
        seen.versions.0 + 2,
        "the version and its restoring"
    );
    assert!(!seen.single);
    Ok(())
}

#[when(
    "`a` and `b`, one in each of two partitions, each hold one object of `nick`, and a client commits a declaration of `nick` single-valued"
)]
fn over_two(w: &mut Steps) -> TestResult {
    let nick = w.iri("nick");
    let scenario = w.iri("partition");
    let cluster = start_own(w.carrier, 2)?;
    let ranges = placement_ranges(&cluster.frontend)?;
    w.tightening = run(async {
        let mut client = client(&cluster.client).await?;
        for partition in 1..=2 {
            let subject = iri_of_partition(&ranges, &scenario, partition)?;
            let write = write_of(&subject, &nick, &string("x"))?;
            let transaction = client.begin(TENANT, false).await?;
            client.write(&transaction, &write).await?;
            let _committed = client.commit(transaction, None).await?;
        }
        let committed = declared(
            &mut client,
            &declaration(DeclaredPredicate::new(&nick).with_single(true)),
        )
        .await?;
        let reports = cluster
            .frontend_log()?
            .lines()
            .filter(|line| line.contains(REPORTED))
            .map(str::to_string)
            .collect();
        let mut described = false;
        for _ in 0..POLLS {
            described = single(&mut client, &nick).await?;
            if described {
                break;
            }
            pause(POLL).await;
        }
        Ok(Seen {
            answered: committed.version(),
            reports,
            single: described,
            ..Seen::default()
        })
    })?;
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "both partitions have reported when the Commit answers the first version of the effective epoch, and Describe reports `nick` single-valued"
)]
fn over_two_answered(w: &mut Steps) {
    let seen = &w.tightening;
    assert_eq!(
        seen.answered & ((1 << 22) - 1),
        0,
        "an epoch's first version"
    );
    for partition in ["partition=1", "partition=2"] {
        assert!(
            seen.reports.iter().any(|line| line.contains(partition)),
            "{partition} reported before the answer: {:?}",
            seen.reports
        );
    }
    assert!(seen.single);
}

/// The wait of the silent scenario, in milliseconds.
const CHECK_MS: u64 = 300;

#[when(
    "a client commits a declaration of `nick` single-valued, and a partition holding the tenant reports nothing within `catalog_check_ms`"
)]
fn silent(w: &mut Steps) -> TestResult {
    let nick = w.iri("nick");
    let spec = Spec::one_partition().frontend("metadata.catalog_check_ms", CHECK_MS);
    let cluster = start_own_as(w.carrier, &spec)?;
    let before = latest_version(&cluster.frontend)?;
    let mut seen = run(async {
        let mut client = client(&cluster.client).await?;
        cluster.signal("storage", Signal::Stop)?;
        let answer = declared(
            &mut client,
            &declaration(DeclaredPredicate::new(&nick).with_single(true)),
        )
        .await;
        let refused = refused_with(answer, ErrorCode::TimedOut);
        // Whatever the answer, a process left stopped would hold the cluster's end up.
        cluster.signal("storage", Signal::Continue)?;
        let (detail, message) = refused?;
        let mut late = false;
        for _ in 0..POLLS {
            late = cluster.frontend_log()?.contains(REPORTED);
            if late {
                break;
            }
            pause(POLL).await;
        }
        assert!(late, "the partition, going on, reports the version late");
        Ok(Seen {
            refusal: Some((ErrorCode::TimedOut, detail, message)),
            single: single(&mut client, &nick).await?,
            ..Seen::default()
        })
    })?;
    seen.versions = (before, latest_version(&cluster.frontend)?);
    w.tightening = seen;
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "the Commit is refused with code 24, `detail` that partition, naming it and the version, the metadata group holds a version restoring the previous declaration, and a report arriving later changes nothing"
)]
fn silent_refused(w: &mut Steps) -> TestResult {
    let seen = &w.tightening;
    let (code, detail, message) = seen.refusal.as_ref().context("the refusal")?;
    assert_eq!(*code, ErrorCode::TimedOut);
    assert_eq!(*detail, 1, "the partition silent");
    let withdrawn = seen.versions.0 + 1;
    assert!(
        message.contains("partition 1") && message.contains(&withdrawn.to_string()),
        "{message}"
    );
    assert_eq!(
        seen.versions.1,
        seen.versions.0 + 2,
        "the version and its restoring"
    );
    assert!(!seen.single, "the late report changed nothing");
    Ok(())
}

#[when(
    "a client's declaration of `name` a property is acknowledged, `a` holding a literal of it, the storage process restarts, and a client writes `<a> <name> <b>`"
)]
fn after_restart(w: &mut Steps) -> TestResult {
    let (a, name, b, w_other) = (w.iri("a"), w.iri("name"), w.iri("b"), w.iri("other"));
    let mut cluster = start_own(w.carrier, 1)?;
    run(async {
        let mut client = client(&cluster.client).await?;
        let write = write_of(&a, &name, &string("Ann"))?;
        let transaction = client.begin(TENANT, false).await?;
        client.write(&transaction, &write).await?;
        let _committed = client.commit(transaction, None).await?;
        let property = DeclaredPredicate::new(&name).with_kind(DeclaredKind::Property);
        let _acknowledged = declared(&mut client, &declaration(property)).await?;
        committed_past(&mut client, &a, &w_other).await
    })?;
    // A stop syncs the store past the version's round and a commit of its epoch, so no replay and no close delivered again takes
    // the version effect: the start alone can.
    cluster.stop("storage")?;
    cluster.restart("storage")?;
    w.tightening = run(async {
        let mut client = client(&cluster.client).await?;
        let object: RequestTerm = iri(&b);
        let forbidden = write_of(&a, &name, &object)?;
        let refusal = forbidden_write(&mut client, &forbidden).await?;
        let pattern = Pattern::of(Some(&a), Some(&name));
        let read = client.begin(TENANT, true).await?;
        let found = read_whole(&mut client, &read, &pattern).await?;
        Ok(Seen {
            refusal: Some(refusal),
            objects: found.rows.len(),
            ..Seen::default()
        })
    })?;
    w.cluster = Some(cluster);
    Ok(())
}

/// Commits `<a> <other> "x"`, run again while the version just acknowledged is still arriving in its partition.
async fn committed_past(client: &mut TestClient, a: &str, other: &str) -> TestResult {
    let write = write_of(a, other, &string("x"))?;
    for _ in 0..POLLS {
        match declared(client, &write).await {
            Ok(_committed) => return Ok(()),
            Err(error)
                if matches!(
                    crate::client_reads::refusal_of(&error),
                    Ok((ErrorCode::Conflict, ..))
                ) =>
            {
                pause(POLL).await;
            }
            Err(error) => return Err(TestError::new(error.to_string())),
        }
    }
    Err(TestError::new(
        "the write behind the version never committed",
    ))
}

/// Writes `forbidden` until its partition, restarted, takes it or refuses it: the refusal.
async fn forbidden_write(
    client: &mut TestClient,
    forbidden: &Write,
) -> TestResult<(ErrorCode, u64, String)> {
    for _ in 0..POLLS {
        let answer = declared(client, forbidden).await;
        match answer {
            Ok(committed) => {
                return Err(TestError::new(format!(
                    "the write landed at {}",
                    committed.version()
                )));
            }
            Err(error) => {
                let refusal = crate::client_reads::refusal_of(&error);
                match refusal {
                    Ok((ErrorCode::Unavailable, ..)) => pause(POLL).await,
                    Ok(refused) => return Ok(refused),
                    // A connection the restart cut.
                    Err(_) => {
                        if !matches!(error.0, ClientErrorEnum::Refused { .. }) {
                            pause(POLL).await;
                        }
                    }
                }
            }
        }
    }
    Err(TestError::new(
        "the restarted partition neither took nor refused the write",
    ))
}

#[then("the write is refused, and `a` holds its literal alone")]
fn write_refused(w: &mut Steps) -> TestResult {
    let (code, _, message) = w.tightening.refusal.as_ref().context("the refusal")?;
    assert_eq!(*code, ErrorCode::Refused, "{message}");
    assert_eq!(w.tightening.objects, 1);
    Ok(())
}

#[when(
    "a client's declaration of a new `name` a property is acknowledged over two partitions, and a client writes an IRI object of `name` to a subject of each"
)]
fn no_term_held(w: &mut Steps) -> TestResult {
    let (name, b, stem) = (w.iri("name"), w.iri("b"), w.iri("subject"));
    let cluster = start_own(w.carrier, 2)?;
    let ranges = placement_ranges(&cluster.frontend)?;
    w.seen.facts = run(async {
        let mut client = client(&cluster.client).await?;
        let property = DeclaredPredicate::new(&name).with_kind(DeclaredKind::Property);
        let _acknowledged = declared(&mut client, &declaration(property)).await?;
        let mut refusals = Vec::new();
        for partition in 1..=2 {
            let subject = iri_of_partition(&ranges, &stem, partition)?;
            let forbidden = write_of(&subject, &name, &iri(&b))?;
            let (code, _, message) = forbidden_write(&mut client, &forbidden).await?;
            refusals.push(format!("{code:?}: {message}"));
        }
        Ok(refusals)
    })?;
    w.cluster = Some(cluster);
    Ok(())
}

#[then("each write is refused")]
fn each_refused(w: &mut Steps) {
    assert_eq!(w.seen.facts.len(), 2, "{:?}", w.seen.facts);
    assert!(
        w.seen.facts.iter().all(|fact| fact.starts_with("Refused")),
        "{:?}",
        w.seen.facts
    );
}
