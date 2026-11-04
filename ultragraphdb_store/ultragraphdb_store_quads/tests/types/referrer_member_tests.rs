/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Epoch};
use ultragraphdb_store_quads::ReferrerMember;
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_a_member_holds_its_group_epoch_and_generation() -> TestResult {
    let group = BucketGroup::new(7)?;
    let epoch = Epoch::new(12)?;
    let generation = NonZeroU64::new(3).context("3 is no zero")?;
    let member = ReferrerMember::new(group, epoch, generation);
    assert_eq!(member.group(), group);
    assert_eq!(member.epoch(), epoch);
    assert_eq!(member.generation(), generation);
    Ok(())
}
