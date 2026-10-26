#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The contract is versioned
  A store records the mapping contract's version it was written under, and a build refuses another major version.

  Scenario: A store of another major version is refused
    When a store written under contract version 2 is opened by a build implementing version 1
    Then opening fails with an error naming contract versions 2 and 1, and nothing is written
