/*
 * Copyright (c) "2026" . Marvin Hansen All Rights Reserved.
 */

//! Test entry point for `src/utils_test`.
//!
//! Separate from `lib_tests` because the module under test only exists with the `test-utils`
//! feature on, and gating the client's own unit tests on that feature would mean a plain
//! `cargo test` ran nothing.
//!
//! The tests/utils_test tree mirrors src/utils_test exactly; each module is registered here so
//! a test file cannot be added without also being wired in.

#[cfg(test)]
mod utils_test;
