/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The system ID's decoder over arbitrary bytes. Sixteen bytes decode to an ID or are refused naming the first
//! invalid field in the decoder's order (the reserved bits, the type prefix, the sequence); any other length is
//! refused naming it; nothing panics. An accepted ID encodes back to the bytes it was decoded from.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_ids::{IdErrorEnum, SystemId};

fn check(data: &[u8]) {
    let decoded = SystemId::decode(data);
    let Ok(bytes) = <[u8; 16]>::try_from(data) else {
        assert_eq!(
            decoded.map_err(|e| e.0),
            Err(IdErrorEnum::Length {
                what: "a system ID",
                expected: 16,
                found: data.len(),
            })
        );
        return;
    };
    let reserved = u32::from_be_bytes([0, bytes[13], bytes[14], bytes[15]]);
    let prefix = u16::from_be_bytes([bytes[0], bytes[1]]);
    let sequence = u64::from_be_bytes([
        bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11], bytes[12],
    ]);
    let expected = if reserved != 0 {
        Some(IdErrorEnum::ReservedBits { bits: reserved })
    } else if prefix == 0 {
        Some(IdErrorEnum::ZeroTypePrefix)
    } else if sequence == 0 {
        Some(IdErrorEnum::ZeroSequence)
    } else {
        None
    };
    match (decoded, expected) {
        (Ok(id), None) => {
            assert_eq!(
                id.to_be_bytes(),
                bytes,
                "an accepted ID encodes back to its bytes"
            );
            assert_eq!(id.to_u128(), u128::from_be_bytes(bytes));
            assert_eq!(SystemId::from_u128(id.to_u128()), Ok(id));
            assert_eq!(id.type_prefix().get(), prefix);
            assert_eq!(id.sequence().get(), sequence);
            assert_eq!(
                id.bucket().get(),
                u32::from_be_bytes([0, bytes[2], bytes[3], bytes[4]])
            );
        }
        (Err(error), Some(variant)) => assert_eq!(error.0, variant),
        (outcome, expected) => panic!("decoded {outcome:?}, expected the refusal {expected:?}"),
    }
}

fuzz_target!(|data: &[u8]| check(data));
