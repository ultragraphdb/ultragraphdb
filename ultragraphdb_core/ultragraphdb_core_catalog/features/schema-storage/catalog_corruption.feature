#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A catalog version that does not read back
  A catalog version the metadata group holds that does not decode refuses the load of its tenant's catalog, by every
  role that loads it, with the corruption error naming the version; nothing is skipped, since a declaration skipped
  would let writes go on under the default declaration.

  Scenario: A catalog version that does not decode refuses the load
    When a tenant's catalog is loaded from a metadata group holding a version that does not decode
    Then the load fails with a corruption error naming the version, and nothing panics
