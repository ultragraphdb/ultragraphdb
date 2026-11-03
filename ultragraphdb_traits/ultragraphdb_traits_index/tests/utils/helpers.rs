/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the tests share: system IDs numbered as Dgraph's tests number their uids.

use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_tool_bdd::TestResult;

/// The ID the tests number `n`: type 1, bucket 0 and sequence `n`, so the IDs sort as their numbers do.
pub fn id(n: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(0)?,
        Sequence::new(n)?,
    ))
}
