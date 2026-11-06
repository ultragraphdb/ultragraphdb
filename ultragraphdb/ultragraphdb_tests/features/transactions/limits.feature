# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: An ended transaction releases what it holds
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request. It shares the one wait of `transaction_ms` the first run runs.

  Scenario: A vanished client's transaction is aborted at its limit
    When a read-write transaction writes `<new> <p> "x"`, creating `<new>`, and its client then sends nothing, its connection staying open
    Then the host aborts the transaction once 10 s have passed, nothing it wrote is held, the dictionary holds no entry it created for `<new>`, and a later transaction writing `<new>` commits
