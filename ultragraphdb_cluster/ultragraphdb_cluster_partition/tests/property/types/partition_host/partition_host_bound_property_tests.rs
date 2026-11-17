/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Drop's catalog check at the round (design D14, `PROTOCOL.md` §6.5): law L4.1.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_cluster_partition::PartitionMessage;
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::{CatalogVersion, Close};
use ultragraphdb_common_storage::error_reply;
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_store_apply::DropRequest;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::one_host::{Host, TENANT, ex, holds, on_host, turned, written};

/// The highest epoch a drawn history reaches: few enough closes that a case runs in milliseconds.
const EPOCH_MAX: u64 = 12;

/// The versions a drawn history holds at most.
const VERSIONS_MAX: u64 = 4;

/// Code 2 (Conflict), kind 5 (Catalog), as `PROTOCOL.md` §6.5 and §9.4 name a stale Drop's refusal.
const CATALOG_CONFLICT: (u16, u64) = (2, 5);

/// What a Drop came to: its refusal's code and detail, if refused, and whether it removed the fact written before it.
type Decided = (Option<(u16, u64)>, bool);

/// The model, from §11.12 and §6.5: the version in effect at `epoch` is the highest whose effective epoch is at or below it,
/// 0 when none is.
fn in_effect(history: &[(u64, u64)], epoch: u64) -> u64 {
    history
        .iter()
        .filter(|(_, effective)| *effective <= epoch)
        .map(|(version, _)| *version)
        .max()
        .unwrap_or(0)
}

/// What a drawn Drop came to on partition 1 after the closes through `through`, each close announcing the versions of
/// `history` that take effect one epoch above it.
fn decided(history: &[(u64, u64)], through: u64, kind: u64, catalog: u64) -> TestResult<Decided> {
    let mut found = None;
    on_host(|host| {
        found = Some(decide(host, history, through, kind, catalog)?);
        Ok(())
    })?;
    found.context("the host ran")
}

fn decide(
    host: &mut Host<'_>,
    history: &[(u64, u64)],
    through: u64,
    kind: u64,
    catalog: u64,
) -> TestResult<Decided> {
    // Written before any version takes effect, in the default graph and in `<g>`: a write executes against the version its
    // partition installed, and a Drop of a predicate or a graph that finds nothing to remove records no drop.
    let _written = host.submit(written("s", "p", "g"), 0)?;
    let _settled = turned(host, 3)?;
    let sender = host
        .mailbox()
        .sender(PartitionId::new(1)?)
        .context("partition 1's sender")?;
    // Epoch 1 is closed already; each later close announces what takes effect at the epoch above it.
    for at in 2..=through {
        let announced = history
            .iter()
            .filter(|(_, effective)| *effective == at + 1)
            .map(|(version, _)| CatalogVersion::new(Tenant::new(TENANT), *version))
            .collect();
        sender.send(PartitionMessage::Close(Close::new(
            Epoch::new(at)?,
            0,
            announced,
            Vec::new(),
        )?))?;
        let _closed = turned(host, 2)?;
    }
    let (predicate, graph) = (ex("p"), ex("g"));
    let drop = match kind {
        1 => DropRequest::Predicate(predicate.as_ref()),
        2 => DropRequest::Graph(oxrdf::GraphNameRef::NamedNode(graph.as_ref())),
        3 => DropRequest::Data,
        _ => DropRequest::Tenant,
    };
    let before = holds(host, "s", "p")?;
    if !before {
        return Err(TestError::new("the fact written is held before the Drop"));
    }
    let ticket = host.commit_drop((TENANT, 1), drop, catalog, (0, 0), 0)?;
    let ended = turned(host, 3)?;
    let outcome = ended
        .iter()
        .find(|o| o.ticket() == ticket)
        .context("the Drop's outcome")?;
    let refused = outcome.outcome().as_ref().err().map(|refusal| {
        let reply = error_reply(refusal);
        (reply.code().number(), reply.detail())
    });
    Ok((refused, !holds(host, "s", "p")?))
}

/// L4.1: over a drawn catalog history of 0 to 4 versions, each with its effective epoch, and a Drop of a drawn kind naming a
/// drawn `catalog` from 0 to one above the highest version, decided in a round of a drawn epoch: a Drop of kind 3 or 4
/// commits exactly when its `catalog` is the version in effect at the round's epoch, and is otherwise refused with code 2,
/// kind 5, removing nothing; a Drop of kind 1 or 2 is not checked.
#[hegel::test]
fn test_a_drop_commits_exactly_under_the_version_in_effect_at_its_round(tc: TestCase) {
    let count = tc.draw(gs::integers::<u64>().min_value(0).max_value(VERSIONS_MAX));
    // Effective epochs strictly rising from 3, the first a close after epoch 1's can announce.
    let mut history = Vec::new();
    let mut last = 2;
    for version in 1..=count {
        let room = EPOCH_MAX - (count - version);
        let effective = tc.draw(gs::integers::<u64>().min_value(last + 1).max_value(room));
        history.push((version, effective));
        last = effective;
    }
    let through = tc.draw(gs::integers::<u64>().min_value(2).max_value(EPOCH_MAX));
    let kind = tc.draw(gs::integers::<u64>().min_value(1).max_value(4));
    let catalog = tc.draw(gs::integers::<u64>().min_value(0).max_value(count + 1));
    let round = through + 1;
    let checked = kind >= 3;
    let commits = !checked || catalog == in_effect(&history, round);
    let found = decided(&history, through, kind, catalog).map_err(|error| error.to_string());
    assert!(found.is_ok(), "the case ran: {found:?}");
    let Ok((refused, changed)) = found else {
        return;
    };
    if commits {
        assert_eq!(
            refused, None,
            "kind {kind}, catalog {catalog}, history {history:?}, round {round}"
        );
    } else {
        assert_eq!(
            refused,
            Some(CATALOG_CONFLICT),
            "kind {kind}, catalog {catalog}, history {history:?}, round {round}"
        );
        assert!(!changed, "a refused Drop removes nothing");
    }
    if checked && commits {
        assert!(
            changed,
            "a Drop of a tenant's data or of everything removes the fact written"
        );
    }
}
