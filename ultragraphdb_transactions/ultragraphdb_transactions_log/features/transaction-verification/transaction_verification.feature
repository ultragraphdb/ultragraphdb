#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Transaction verification
  The regression cases of the fuzz targets, replayed hermetically under Bazel. The log's scan holds the first finding:
  a segment whose scan ended past its bytes, minimized into `testdata/fuzz_regressions/scan/` and replayed here as the
  fuzz target `scan` replays it, and by a unit test of the scan. The requirement's other scenarios run `lake` and
  `cargo-fuzz`, which are not hermetic: `scripts/lean.sh` checks that the model builds without gaps, and
  `scripts/fuzz.sh` with the fuzzers' audit checks the fuzzing scenarios, their runs recorded in the change's
  `notes/group-11.md`.

  Scenario: A fuzzer finding becomes a regression test
    When a fuzz target finds an input that panics a decoder, and the decoder is fixed
    Then the minimized input is committed beside the crate's tests, and a unit test replaying it passes under Cargo and Bazel
