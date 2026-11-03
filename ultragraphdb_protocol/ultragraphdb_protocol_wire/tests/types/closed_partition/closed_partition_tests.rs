/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ClosedPartition;

#[test]
fn test_a_partition_encodes_its_fields_and_zero_bytes() {
    let record = ClosedPartition::new(0x0403_0201, 0x0F0E_0D0C_0B0A_0908);
    assert_eq!(record.partition(), 0x0403_0201);
    assert_eq!(record.epoch(), 0x0F0E_0D0C_0B0A_0908);
    assert_eq!(
        record.encode(),
        [1, 2, 3, 4, 0, 0, 0, 0, 8, 9, 10, 11, 12, 13, 14, 15]
    );
}

#[test]
fn test_the_extremes_encode() {
    assert_eq!(ClosedPartition::new(0, 0).encode(), [0; 16]);
    let most = ClosedPartition::new(u32::MAX, u64::MAX).encode();
    assert_eq!(&most[..4], &[0xFF; 4]);
    assert_eq!(&most[4..8], &[0; 4]);
    assert_eq!(&most[8..], &[0xFF; 8]);
}
