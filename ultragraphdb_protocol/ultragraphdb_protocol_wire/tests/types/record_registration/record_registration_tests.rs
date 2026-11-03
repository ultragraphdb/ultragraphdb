/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;

use crate::types::write_set_record::write_set_record_tests::{registration, sid};

#[test]
fn test_a_registration_keeps_its_entity_group_and_generation() -> TestResult {
    let entity = sid(1, 9, 0x40)?;
    let r = registration(entity, 1, 3)?;
    assert_eq!(r.entity(), entity);
    assert_eq!(r.group().get(), 1);
    assert_eq!(r.generation(), 3);
    assert_ne!(r, registration(entity, 1, 4)?);
    assert_ne!(r, registration(entity, 2, 3)?);
    assert_ne!(r, registration(sid(1, 10, 0x40)?, 1, 3)?);
    Ok(())
}

#[test]
fn test_a_registration_is_laid_out_as_the_table_states() -> TestResult {
    // PROTOCOL.md §12.2: the entity's 16 bytes, big-endian (prefix 1, sequence 9, bucket 0x40, 3 reserved zero
    // bytes); `group` u32 little-endian; 4 zero bytes; `generation` u64 little-endian.
    let r = registration(sid(1, 9, 0x40)?, (1 << 18) - 1, u64::MAX)?;
    let mut expected = vec![
        0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 9, 0x00, 0x00, 0x40, 0, 0, 0,
    ];
    expected.extend_from_slice(&[0xFF, 0xFF, 0x03, 0x00]);
    expected.extend_from_slice(&[0; 4]);
    expected.extend_from_slice(&[0xFF; 8]);
    assert_eq!(r.encode().to_vec(), expected);
    let least = registration(sid(1, 9, 0x40)?, 0, 1)?;
    assert_eq!(
        least.encode()[16..],
        [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]
    );
    Ok(())
}
