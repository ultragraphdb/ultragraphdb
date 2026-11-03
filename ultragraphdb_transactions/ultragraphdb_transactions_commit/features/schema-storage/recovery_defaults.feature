#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Writes to an unknown predicate create its schema, across a crash
  A first write of an undeclared predicate asks the metadata group to record its default entry, a command no write
  waits for; replay asks again for every undeclared predicate its replayed rounds wrote, which the command's finding an
  existing record makes harmless. The other scenarios of the requirement run in ultragraphdb_store_apply's features.

  Scenario: Recovery asks again for a default entry
    When a commit first writes the undeclared `alias` and the process is killed before its default-entry command is sent, and the partition recovers
    Then replay sends the command for `alias`, and once it is answered the metadata group holds its record
