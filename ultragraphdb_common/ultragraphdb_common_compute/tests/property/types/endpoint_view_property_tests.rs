/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An endpoint core's view: laws L5.1, L5.2 and L5.4 of `notes/laws.md` (`PROTOCOL.md` §4.8, §11.12, *Epochs*).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use compio::runtime::Runtime;
use hegel::TestCase;
use hegel::generators as gs;
use oxrdf::NamedNode;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_compute::{EndpointView, HoldOutcome};
use ultragraphdb_core_catalog::{CatalogContents, describe_of};
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};

const TENANTS: [u64; 3] = [0, 1, 2];

/// A point in time from the runtime's timer, whose first tick is now.
fn start() -> Option<Instant> {
    let runtime = Runtime::new().ok()?;
    Some(runtime.block_on(async { compio::time::interval(Duration::from_secs(1)).tick().await }))
}

fn map() -> Option<PlacementMap> {
    PlacementMap::even(2, NodeId::new(1)).ok()
}

/// L5.1: under any sequence of closes, announcements, Declare replies, Catalog reads and subscriptions opened again, the
/// latest closed epoch and each tenant's version never fall; after a subscription opened again the view answers nothing
/// until Placement and each tenant's current version are read, and then holds what was read.
#[hegel::test(test_cases = 60)]
fn test_the_view_never_goes_back(tc: TestCase) {
    let mut view = EndpointView::new();
    let Some(placement) = map() else {
        return;
    };
    let mut closed = 0u64;
    let mut versions: BTreeMap<u64, u64> = BTreeMap::new();
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(30));
    for _ in 0..steps {
        let step = tc.draw(gs::integers::<u8>().min_value(0).max_value(4));
        match step {
            0 => {
                let skip = tc.draw(gs::integers::<u64>().min_value(0).max_value(4));
                let epoch = closed.saturating_add(1).saturating_add(skip);
                let tenant = tc.draw(gs::sampled_from(TENANTS.to_vec()));
                let version = tc.draw(gs::integers::<u64>().min_value(1).max_value(9));
                let _applied = view.close(epoch, 0, &[(tenant, version)]);
                closed = closed.max(epoch);
                let held = versions.entry(tenant).or_insert(0);
                *held = (*held).max(version);
            }
            1 => {
                let tenant = tc.draw(gs::sampled_from(TENANTS.to_vec()));
                let version = tc.draw(gs::integers::<u64>().min_value(1).max_value(9));
                view.declared(tenant, version, 0);
                let held = versions.entry(tenant).or_insert(0);
                *held = (*held).max(version);
            }
            2 => {
                view.reopened();
                let epoch = closed + tc.draw(gs::integers::<u64>().min_value(0).max_value(4));
                let _applied = view.close(epoch.max(1), 0, &[]);
                closed = closed.max(epoch.max(1));
                assert!(!view.answers(), "nothing answered before the reads");
                for tenant in versions.keys() {
                    if versions.get(tenant).is_some_and(|v| *v > 0) {
                        assert!(view.wants_catalogs().contains(tenant), "{tenant}");
                    }
                }
            }
            3 => {
                view.placement(placement.clone(), 1, TENANTS.to_vec());
            }
            _ => {
                for tenant in view.wants_catalogs() {
                    let held = versions.get(&tenant).copied().unwrap_or(0);
                    let read = held + tc.draw(gs::integers::<u64>().min_value(0).max_value(2));
                    let bytes = CatalogContents::new(tenant, read.max(1), Vec::new()).encode();
                    if let Ok(bytes) = bytes {
                        assert!(view.catalog(tenant, read.max(1), 0, &bytes).is_ok());
                        let _previous = versions.insert(tenant, held.max(read.max(1)));
                    }
                }
            }
        }
        assert_eq!(view.latest_closed(), closed);
        for (tenant, version) in &versions {
            assert_eq!(view.current_version(*tenant), *version, "{tenant}");
        }
        if view.answers() {
            assert!(view.wants_catalogs().is_empty() && !view.wants_placement());
        }
    }
}

/// L5.2: a Begin naming a token whose epoch is 0 to 3 above the view's open epoch is answered at once when within it, else
/// answered once the close before it arrives, a close arrived at the deadline taken first, and timed out at its deadline
/// otherwise; never both, never twice.
#[hegel::test(test_cases = 60)]
fn test_a_held_begin_agrees_with_its_model(tc: TestCase) {
    let (Some(origin), Some(placement)) = (start(), map()) else {
        return;
    };
    let mut view = EndpointView::new();
    let _applied = view.close(7, 0, &[]);
    view.placement(placement, 1, vec![0]);
    let mut closed = 7u64;
    let mut held: BTreeMap<u64, (u64, Instant)> = BTreeMap::new();
    let mut decided: BTreeMap<u64, HoldOutcome> = BTreeMap::new();
    let mut now = origin;
    for ticket in 0..tc.draw(gs::integers::<u64>().min_value(1).max_value(12)) {
        match tc.draw(gs::integers::<u8>().min_value(0).max_value(2)) {
            0 => {
                let ahead = tc.draw(gs::integers::<u64>().min_value(0).max_value(3));
                let epoch = closed + 1 + ahead;
                let wait = tc.draw(gs::integers::<u64>().min_value(1).max_value(5));
                let deadline = now + Duration::from_millis(wait);
                let at_once = view.hold(ticket, epoch, deadline);
                if ahead == 0 {
                    assert_eq!(at_once, Some(HoldOutcome::Answered));
                    let _none = decided.insert(ticket, HoldOutcome::Answered);
                } else {
                    assert_eq!(at_once, None);
                    let _none = held.insert(ticket, (epoch, deadline));
                }
            }
            1 => {
                closed += 1;
                let _applied = view.close(closed, 0, &[]);
            }
            _ => {
                now +=
                    Duration::from_millis(tc.draw(gs::integers::<u64>().min_value(0).max_value(3)));
            }
        }
        let released = view.released(now);
        let mut expected: Vec<(u64, HoldOutcome)> = Vec::new();
        held.retain(|ticket, (epoch, deadline)| {
            if *epoch <= closed + 1 {
                expected.push((*ticket, HoldOutcome::Answered));
                false
            } else if now >= *deadline {
                expected.push((*ticket, HoldOutcome::TimedOut));
                false
            } else {
                true
            }
        });
        let mut got = released;
        got.sort_by_key(|(ticket, _)| *ticket);
        expected.sort_by_key(|(ticket, _)| *ticket);
        assert_eq!(got, expected);
        for (ticket, outcome) in expected {
            assert!(decided.insert(ticket, outcome).is_none(), "decided twice");
        }
    }
}

/// L5.4: a version's schema id is a function of its bytes: two views reading the same bytes hold one id, and it is the one
/// `describe_of` gives the version's declarations.
#[hegel::test(test_cases = 30)]
fn test_a_schema_id_is_a_function_of_the_bytes(tc: TestCase) {
    let count = tc.draw(gs::integers::<usize>().min_value(0).max_value(6));
    let declarations: Vec<Declaration> = (0..count)
        .filter_map(|_| {
            let name: String = tc.draw(gs::text().alphabet("abcdefgh").min_size(1).max_size(8));
            NamedNode::new(format!("http://ex.org/{name}"))
                .ok()
                .map(|iri| Declaration::Predicate(PredicateDeclaration::new(iri)))
        })
        .collect();
    let version = tc.draw(gs::integers::<u64>().min_value(1).max_value(9));
    let Ok(bytes) = CatalogContents::new(1, version, declarations).encode() else {
        return;
    };
    let Ok(decoded) = CatalogContents::decode(1, version, &bytes) else {
        return;
    };
    let expected = describe_of(1, 0, decoded.declarations(), std::iter::empty()).schema_id();
    let mut first = EndpointView::new();
    let mut second = EndpointView::new();
    assert!(first.catalog(1, version, 0, &bytes).is_ok());
    assert!(second.catalog(1, version, 0, &bytes).is_ok());
    assert_eq!(first.schema_id(1), Some(expected));
    assert_eq!(second.schema_id(1), Some(expected));
}
