/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The body layout's reader over arbitrary bytes. The first byte chooses the section count, 1, 3, 7 or 9, and the
//! fixed part, 0, 8, 16 or 24 bytes; the rest is the body. A body is read or refused with a `BodyError`; nothing
//! panics. The layout has one encoding, so an accepted body written again from its fixed part and sections gives the
//! bytes it was read from.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_body::{Sections, write_sections};

/// `body` read as `N` sections after `fixed` bytes, and written again when accepted.
fn check<const N: usize>(body: &[u8], fixed: usize) {
    if let Ok(sections) = Sections::<N>::read(body, fixed) {
        let parts: Vec<&[u8]> = (0..N).map(|index| sections.get(index)).collect();
        assert_eq!(
            write_sections(sections.fixed(), &parts),
            body,
            "an accepted body written again gives its bytes"
        );
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((choice, body)) = data.split_first() else {
        return;
    };
    let fixed = usize::from((choice >> 2) & 3) * 8;
    match choice & 3 {
        0 => check::<1>(body, fixed),
        1 => check::<3>(body, fixed),
        2 => check::<7>(body, fixed),
        _ => check::<9>(body, fixed),
    }
});
