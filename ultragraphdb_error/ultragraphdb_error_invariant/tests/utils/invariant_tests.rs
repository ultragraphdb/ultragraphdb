/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `invariant` in both builds: the debug build's tests run in `ultragraphdb_error_invariant_tests`, the release
//! build's in `ultragraphdb_error_invariant_release_tests`, which compiles the crate and this suite with debug
//! assertions off (`BUILD.bazel`); `cargo test --release` runs the release half too.

use ultragraphdb_error_invariant::invariant;

const NAME: &str = "rounds are appended in position order";

#[test]
fn test_an_invariant_that_holds_passes() {
    assert_eq!(invariant(true, NAME), Ok(()));
}

/// A debug build, as tests, the simulator and the fuzzers run: a violation panics with the invariant's name, at the
/// caller's location, which the panic hook records.
#[cfg(debug_assertions)]
#[test]
fn test_a_violation_panics_at_the_caller_in_a_debug_build() {
    use std::panic;
    use std::sync::{Arc, Mutex};

    let seen: Arc<Mutex<Option<(String, u32)>>> = Arc::default();
    let record = Arc::clone(&seen);
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if let (Some(location), Ok(mut seen)) = (info.location(), record.lock()) {
            *seen = Some((location.file().to_string(), location.line()));
        }
    }));
    let line = line!() + 1;
    let caught = panic::catch_unwind(|| invariant(false, NAME));
    panic::set_hook(previous);
    let payload = caught.expect_err("a violation panics in a debug build");
    let message = payload.downcast_ref::<String>().map(String::as_str);
    assert_eq!(
        message,
        Some("invariant violated: rounds are appended in position order")
    );
    let seen = seen.lock().expect("the hook released the lock").clone();
    assert_eq!(
        seen,
        Some((file!().to_string(), line)),
        "the panic names the caller's location, not the check's"
    );
}

/// A release build: a violation is returned, naming the invariant, and nothing panics.
#[cfg(not(debug_assertions))]
#[test]
fn test_a_violation_is_returned_in_a_release_build() {
    assert_eq!(invariant(false, NAME), Err(InvariantError::Violated(NAME)));
    assert_eq!(
        invariant(false, "a session has one owner"),
        Err(InvariantError::Violated("a session has one owner"))
    );
}
