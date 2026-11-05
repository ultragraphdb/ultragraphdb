/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, CatalogAnnouncement, EpochClose};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden EpochClose.
pub fn golden_epoch_close() -> EpochClose {
    let mut close = EpochClose::new(42, 1_798_761_600_123_456).with_more();
    for (tenant, version) in [(3, 11), (3, 12), (7, 4)] {
        close.push_announcement(CatalogAnnouncement::new(tenant, version));
    }
    close.push_eviction(std::array::from_fn(|i| 0x11 + i as u8));
    close.push_eviction(std::array::from_fn(|i| 0x31 + i as u8));
    close
}

#[test]
fn test_the_builder_lays_out_the_golden_epoch_close() -> TestResult {
    let golden = golden_role("epoch_close")?;
    assert_eq!(
        golden_epoch_close().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

#[test]
fn test_the_getters_give_back_what_the_constructor_and_pushes_took() {
    let close = golden_epoch_close();
    assert_eq!(
        (close.epoch(), close.time(), close.more()),
        (42, 1_798_761_600_123_456, true)
    );
    assert_eq!(
        close.announcements(),
        [
            CatalogAnnouncement::new(3, 11),
            CatalogAnnouncement::new(3, 12),
            CatalogAnnouncement::new(7, 4)
        ]
        .as_slice()
    );
    let first: [u8; 16] = std::array::from_fn(|i| 0x11 + i as u8);
    let second: [u8; 16] = std::array::from_fn(|i| 0x31 + i as u8);
    assert_eq!(close.evictions(), [first, second].as_slice());
    assert_eq!(close.placement(), 0);
    // Without `with_more`, no frame follows; nothing pushed, nothing held.
    let last = EpochClose::new(0, 0);
    assert_eq!((last.epoch(), last.time(), last.more()), (0, 0, false));
    assert!(last.announcements().is_empty() && last.evictions().is_empty());
    assert_eq!(last.placement(), 0);
    for version in [1, 2, u64::MAX] {
        assert_eq!(
            EpochClose::new(0, 0).with_placement(version).placement(),
            version
        );
    }
}

#[test]
fn test_the_placement_version_is_laid_out_at_offset_24() -> TestResult {
    let golden = golden_role("epoch_close")?;
    let plain = golden_epoch_close().frame(golden.header())?;
    let placed = golden_epoch_close()
        .with_placement(0x0102_0304_0506_0708)
        .frame(golden.header())?;
    // The command header follows the 128-byte frame header; only `placement`'s 8 bytes differ, little-endian.
    let differing: Vec<usize> = (0..plain.len())
        .filter(|&i| plain.get(i) != placed.get(i))
        .collect();
    let header_differing: Vec<usize> = differing
        .iter()
        .copied()
        .filter(|&i| (128..256).contains(&i))
        .collect();
    assert_eq!(header_differing, (152..160).collect::<Vec<_>>());
    assert_eq!(
        placed.get(152..160),
        Some([8u8, 7, 6, 5, 4, 3, 2, 1].as_slice())
    );
    Ok(())
}
