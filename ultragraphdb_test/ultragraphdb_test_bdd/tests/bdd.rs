/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, given, then, when};
use ultragraphdb_test_bdd::{compare_bytes, features_dir};

#[derive(Debug, Default, World)]
struct HarnessWorld {
    expected: Vec<u8>,
    input: String,
    compared: Option<Result<(), String>>,
}

fn parse_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}

#[given(expr = "the golden bytes {string} for the input {string}")]
fn golden_bytes(w: &mut HarnessWorld, bytes: String, input: String) {
    w.expected = parse_hex(&bytes);
    w.input = input;
}

#[when(expr = "the output {string} is compared with them")]
fn compare(w: &mut HarnessWorld, bytes: String) {
    w.compared = Some(compare_bytes(&w.input, &w.expected, &parse_hex(&bytes)));
}

#[then(expr = "the comparison fails and reports {string}, {string} and {string}")]
fn compare_fails(w: &mut HarnessWorld, input: String, expected: String, actual: String) {
    let err = w.compared.clone().unwrap().unwrap_err();
    assert!(
        err.contains(&input) && err.contains(&expected) && err.contains(&actual),
        "{err}"
    );
}

fn main() {
    futures::executor::block_on(
        HarnessWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit(features_dir()),
    );
}
