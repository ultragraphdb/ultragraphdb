/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the property tests draw: settings' values in the shapes a settings file holds them, each from characters a TOML
//! basic string takes without an escape, so the text a test writes is the value it expects back.

use hegel::TestCase;
use hegel::generators as gs;

/// The characters of a host's label or a path's component.
const NAME_CHARACTERS: &str = "abcdefghijklmnopqrstuvwxyz0123456789-_";

/// The characters of a setting's name (role-binaries, "Configuration is explicit and validated at start").
pub(crate) const KEY_CHARACTERS: &str = "abcdefghijklmnopqrstuvwxyz0123456789_";

/// A number from `min` to `max`.
pub(crate) fn within(tc: &TestCase, min: u64, max: u64) -> u64 {
    tc.draw(gs::integers::<u64>().min_value(min).max_value(max))
}

/// A word of 1 to `max` characters of `characters`.
pub(crate) fn word(tc: &TestCase, characters: &str, max: usize) -> String {
    tc.draw(gs::text().alphabet(characters).min_size(1).max_size(max))
}

/// An absolute path of 1 to 4 components.
pub(crate) fn path(tc: &TestCase) -> String {
    let count = within(tc, 1, 4);
    (0..count)
        .map(|_| format!("/{}", word(tc, NAME_CHARACTERS, 10)))
        .collect()
}

/// An address as a settings file writes one: `tcp://host:port`, `unix:///path`, or a bare path.
pub(crate) fn address(tc: &TestCase) -> String {
    match within(tc, 0, 2) {
        0 => format!(
            "tcp://{}:{}",
            word(tc, NAME_CHARACTERS, 12),
            within(tc, 1, u64::from(u16::MAX))
        ),
        1 => format!("unix://{}", path(tc)),
        _ => path(tc),
    }
}

/// Whether a drawn coin came up.
pub(crate) fn coin(tc: &TestCase) -> bool {
    tc.draw(gs::booleans())
}

/// A value of a `u64` setting: from 0 to `i64::MAX`, the integers TOML holds that fit the field.
pub(crate) fn u64_setting(tc: &TestCase) -> u64 {
    within(tc, 0, i64::MAX.unsigned_abs())
}

/// A value of a `u32` setting.
pub(crate) fn u32_setting(tc: &TestCase) -> u32 {
    tc.draw(gs::integers::<u32>())
}
