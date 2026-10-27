#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Port testing harness
  Byte-exact behaviour is checked against golden vectors from the Go source.

  Scenario: A golden mismatch names the input
    Given the golden bytes "00 01" for the input "x"
    When the output "00 02" is compared with them
    Then the comparison fails and reports "x", "00 01" and "00 02"
