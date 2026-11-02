/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The version's decoder over arbitrary bytes. Eight bytes decode to a version, any other length is refused naming
//! it, and nothing panics. An accepted version encodes back to its bytes, its epoch and sequence rebuild it, and it is
//! a commit version exactly when its sequence is even.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_ids::{Epoch, IdErrorEnum, Version};

fn check(data: &[u8]) {
    let decoded = Version::decode(data);
    let Ok(bytes) = <[u8; 8]>::try_from(data) else {
        assert_eq!(
            decoded.map_err(|e| e.0),
            Err(IdErrorEnum::Length {
                what: "a version",
                expected: 8,
                found: data.len(),
            })
        );
        return;
    };
    let version = match decoded {
        Ok(version) => version,
        Err(error) => panic!("eight bytes refused: {error:?}"),
    };
    let value = u64::from_be_bytes(bytes);
    assert_eq!(
        version.to_be_bytes(),
        bytes,
        "an accepted version encodes back to its bytes"
    );
    assert_eq!(version.to_u64(), value);
    assert_eq!(Version::from_u64(value), version);
    assert_eq!(Ok(version.epoch()), Epoch::new(value >> 22));
    assert_eq!(u64::from(version.sequence()), value & 0x3F_FFFF);
    assert_eq!(version.is_commit(), value % 2 == 0);
    let rebuilt = Version::commit(version.epoch(), version.sequence());
    if version.is_commit() {
        assert_eq!(rebuilt, Ok(version));
    } else {
        assert_eq!(
            rebuilt.map_err(|e| e.0),
            Err(IdErrorEnum::OddCommitSequence {
                sequence: version.sequence()
            })
        );
    }
}

fuzz_target!(|data: &[u8]| check(data));
