/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The string list's reader over arbitrary bytes. The first byte is the length of the `ends` section, the bytes after
//! it the section, and the rest the text. A list is read or refused with a `BodyError`; nothing panics. A list has one
//! encoding, so an accepted list written again string by string gives the two sections it was read from.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_body::{StringList, StringWriter};

fuzz_target!(|data: &[u8]| {
    let Some((len, rest)) = data.split_first() else {
        return;
    };
    let Some((ends, text)) = rest.split_at_checked(usize::from(*len)) else {
        return;
    };
    if let Ok(list) = StringList::read((0, 1), ends, text) {
        let mut writer = StringWriter::default();
        for string in list.iter() {
            let _index: u32 = writer.push(string);
        }
        assert_eq!(list.iter().count(), list.len(), "every string is read");
        assert_eq!(
            writer.sections(),
            (ends.to_vec(), text.to_vec()),
            "an accepted list written again gives its sections"
        );
    }
});
