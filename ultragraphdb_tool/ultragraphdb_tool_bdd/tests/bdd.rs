/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The harness's own scenarios: a golden mismatch is reported naming the input and both values.

use std::num::ParseIntError;
use std::str::FromStr;

use cucumber::{World, given, then, when};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult, compare_bytes, features_dir};

#[derive(Debug, Default, World)]
struct HarnessWorld {
    expected: Vec<u8>,
    actual: Vec<u8>,
    input: String,
    compared: Option<TestResult>,
}

/// Bytes a step names in hex, `0a ff`.
struct HexBytes(Vec<u8>);

impl FromStr for HexBytes {
    type Err = ParseIntError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16))
            .collect::<Result<_, _>>()
            .map(HexBytes)
    }
}

#[given(expr = "the golden bytes {string} for the input {string}")]
fn golden_bytes(w: &mut HarnessWorld, bytes: HexBytes, input: String) {
    w.expected = bytes.0;
    w.input = input;
}

#[when(expr = "the output {string} is compared with them")]
fn compare(w: &mut HarnessWorld, bytes: HexBytes) {
    w.actual = bytes.0;
    w.compared = Some(compare_bytes(&w.input, &w.expected, &w.actual));
}

#[then(expr = "the comparison fails and reports {string}, {string} and {string}")]
fn compare_fails(w: &mut HarnessWorld, reported: &[String]) -> TestResult {
    let Err(err) = w.compared.clone().context("a comparison ran")? else {
        return Err(TestError::new("the comparison failed"));
    };
    // Each value must be reported on its own line, so swapping expected and actual cannot pass.
    let lines: Vec<&str> = err.reason().lines().collect();
    assert_eq!(
        lines,
        [
            format!("golden mismatch for input {:?}", reported[0]),
            format!("  expected: {}", reported[1]),
            format!("  actual:   {}", reported[2]),
        ],
        "{err}"
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        HarnessWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit(features_dir()?),
    );
    Ok(())
}
