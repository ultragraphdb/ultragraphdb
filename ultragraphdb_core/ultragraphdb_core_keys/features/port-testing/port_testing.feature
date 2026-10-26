#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Keys are checked against their specification
  The quad layout is the port's own: the key tests read their specification's scenarios, no golden file from Dgraph.

  Scenario: Keys carry no golden vector
    When the key tests run
    Then they check the quad layout against its specification and read no golden file generated from Dgraph
