/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a resume key a client sent back: decoding never panics, and a key it accepts is the one its fields build.
//!
//! Random bytes almost never carry the checksum, so each input is checked twice: as given, and with its first eight bytes
//! replaced by the checksum of the rest, which reaches the rules behind it.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{ResumeKey, ResumeKeyView, xxh3_64};

fn check(data: &[u8]) {
    let Ok(view) = ResumeKeyView::read(data) else {
        return;
    };
    let key = ResumeKey::new(
        view.pattern(),
        view.scope(),
        view.terms().to_table(),
        view.row(),
    );
    assert_eq!(key.encode(), data, "a key has one encoding");
}

fuzz_target!(|data: &[u8]| {
    check(data);
    if let Some(rest) = data.get(8..) {
        let mut sealed = xxh3_64(rest).to_le_bytes().to_vec();
        sealed.extend_from_slice(rest);
        check(&sealed);
    }
});
