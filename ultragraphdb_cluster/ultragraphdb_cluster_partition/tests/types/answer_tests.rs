/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_cluster_partition::Answer;
use ultragraphdb_core_ids::{Epoch, SystemId};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_an_answer_keeps_its_fields() -> TestResult {
    let entity = SystemId::from_u128((1 << 112) | (9 << 48) | (5 << 24))?;
    let generation = NonZeroU64::new(4);
    let answer = Answer::new(entity, generation, Epoch::new(11)?);
    assert_eq!(
        (answer.entity(), answer.generation(), answer.close()),
        (entity, generation, Epoch::new(11)?)
    );
    let predicate = Answer::new(entity, None, Epoch::ZERO);
    assert_eq!(predicate.generation(), None);
    Ok(())
}
