# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A transaction bound to a partition ends with its limit or its connection

  Scenario: A vanished client's transaction is aborted at its limit
    When a read-write transaction writes `<new> <p> "x"`, creating `<new>`, and its client then sends nothing, its connection staying open
    Then the host aborts the transaction once 10 s have passed, nothing it wrote is held, the dictionary holds no entry it created for `<new>`, and a later transaction writing `<new>` commits

  Scenario: A closed connection aborts its transaction at once
    When a read-write transaction creates `<new>` and the connection carrying it closes after 1 s
    Then the host aborts the transaction then, not at its limit, and its entry for `<new>` is released
