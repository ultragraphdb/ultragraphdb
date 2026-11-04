/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_common_config::ConfigRecord;

// Whatever a round's body holds is a record or a refusal, and a record encodes back to the bytes it was read from, so what
// is stored is what is replayed.
fuzz_target!(|bytes: &[u8]| {
    if let Ok(record) = ConfigRecord::decode(bytes) {
        assert_eq!(record.encode(), bytes);
    }
});
