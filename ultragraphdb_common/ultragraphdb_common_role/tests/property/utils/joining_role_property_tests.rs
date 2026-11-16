/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The system group's address a role dials (design D2): law L2.5.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_role::system_address;
use ultragraphdb_common_service::{RoleIdentity, RoleStart};

/// The first character of a host's label: a letter, so no label reads as a number.
const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";

/// The characters after a label's first, and of a socket path's components.
const LETTERS_AND_DIGITS: &str = "abcdefghijklmnopqrstuvwxyz0123456789";

/// A word of a letter followed by up to `rest` letters and digits.
fn word(tc: &TestCase, rest: usize) -> String {
    let first: String = tc.draw(gs::text().alphabet(LETTERS).min_size(1).max_size(1));
    let tail: String = tc.draw(gs::text().alphabet(LETTERS_AND_DIGITS).max_size(rest));
    format!("{first}{tail}")
}

/// A number from `min` to `max`.
fn within(tc: &TestCase, min: u64, max: u64) -> u64 {
    tc.draw(gs::integers::<u64>().min_value(min).max_value(max))
}

/// An address on either carrier, as `PROTOCOL.md` §11.12 writes one: `tcp://host:port`, the host 1 to 3 DNS labels and
/// the port 1 to 65,535, or `unix:///path` of 1 to 3 components.
fn address(tc: &TestCase) -> String {
    if tc.draw(gs::booleans()) {
        let labels: Vec<String> = (0..within(tc, 1, 3)).map(|_| word(tc, 8)).collect();
        format!(
            "tcp://{}:{}",
            labels.join("."),
            within(tc, 1, u64::from(u16::MAX))
        )
    } else {
        let components: String = (0..within(tc, 1, 3))
            .map(|_| format!("/{}", word(tc, 10)))
            .collect();
        format!("unix://{components}")
    }
}

/// L2.5: a `cluster.system_address` drawn on either carrier, in a frontend's, a compute instance's or a storage
/// instance's settings, is the address `system_address` gives.
#[hegel::test]
fn test_the_system_address_is_the_address_drawn(tc: TestCase) {
    let drawn = address(&tc);
    let role = tc.draw(gs::sampled_from(vec!["frontend", "compute", "storage"]));
    let text = format!(
        "[observability]\naddress = \"tcp://127.0.0.1:1\"\n[cluster]\nsystem_address = \"{drawn}\"\n"
    );
    let given = RoleStart::from_settings(text.as_bytes(), RoleIdentity::new("ci", role, 1))
        .ok()
        .and_then(|start| system_address(&start).ok())
        .map(|address| address.to_string());
    assert_eq!(given, Some(drawn));
}
