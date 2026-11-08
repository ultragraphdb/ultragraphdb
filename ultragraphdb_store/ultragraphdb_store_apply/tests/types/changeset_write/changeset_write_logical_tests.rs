/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The derivation of a logged record: a statement whose predicate's ID names a blank node, which no write logs, is
//! refused as the store's corruption naming that ID.

use ultragraphdb_core_keys::DEFAULT_GRAPH;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum, LogicalQuad, LogicalWrites};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::changesets::{TENANT, changesets, reverse, uid, write};

#[test]
fn test_a_logged_predicate_naming_a_blank_node_is_refused_as_corruption() -> TestResult {
    let a = changesets()?;
    write(&a, &["_:x <knows> <b>", "<a> <knows> <b>"])?;
    let blank = *reverse(&a, "knows", "b")?
        .iter()
        .find(|id| uid(&a, "a").is_ok_and(|a| a != **id))
        .context("the blank subject")?;
    let quad = LogicalQuad::new(
        DEFAULT_GRAPH,
        uid(&a, "a")?,
        blank,
        EncodedTerm::id(uid(&a, "b")?),
        None,
    );
    let refused = a
        .prepare_logical(
            TENANT,
            &LogicalWrites::new(vec![quad], Vec::new(), Vec::new()),
            &[],
        )
        .err()
        .context("refused")?;
    assert_eq!(
        refused,
        ChangesetError(ChangesetErrorEnum::Store(StoreError::corrupted(
            blank.to_be_bytes().to_vec(),
            "a logged statement's predicate names no IRI".to_string(),
        )))
    );
    Ok(())
}
