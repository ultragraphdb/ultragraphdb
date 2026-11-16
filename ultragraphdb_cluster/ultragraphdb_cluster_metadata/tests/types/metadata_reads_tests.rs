/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_log::Position;

use crate::utils::harness::{block_on, fresh_with, sync, verifier};

/// While a sync is in flight, the reads say whether the registry holds a session, which principal it is, and how many sessions it
/// holds from every command appended: a registration appended and not durable is held for its principal, and the session it
/// evicted is not.
#[test]
fn test_reads_answer_the_registry_from_every_command_appended() -> TestResult {
    let (_storage, mut group) = fresh_with(MetadataConfig::new().with_sessions_max(2)?)?;
    let _: (u64, Position) =
        group.create_principal("ann", verifier("ann's password!", 1)?, false)?;
    let _: (u64, Position) =
        group.create_principal("bob", verifier("bob's password!", 2)?, false)?;
    sync(&mut group)?;
    let (ann, bob) = (1, 2);
    assert!(group.register_session(1, "ann", |_| false)?.0.is_empty());
    assert!(group.register_session(2, "ann", |_| false)?.0.is_empty());
    sync(&mut group)?;
    assert_eq!(group.reads().sessions(), 2);
    assert_eq!(
        group.register_session(3, "bob", |_| false)?.0,
        [1],
        "the oldest evicted"
    );
    let (syncing, reads) = group.syncing();
    assert_eq!(
        reads.session_principal(3),
        Some(bob),
        "appended, not durable"
    );
    assert_eq!(reads.session_principal(2), Some(ann));
    assert_eq!(reads.session_principal(1), None, "its eviction appended");
    assert_eq!(reads.session_principal(9), None, "never registered");
    assert_eq!(reads.sessions(), 2);
    assert_eq!(
        reads.state().sessions(),
        2,
        "the durable state still holds 1 and 2"
    );
    let durable = block_on(syncing)??;
    group.synced(durable)?;
    assert_eq!(group.reads().session_principal(3), Some(bob));
    assert_eq!(group.reads().sessions(), 2);
    Ok(())
}
