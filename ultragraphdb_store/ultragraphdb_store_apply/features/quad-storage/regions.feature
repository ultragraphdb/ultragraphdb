#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Every key belongs to one region
  A key's region is computable from the key alone, and two tenants sharing a bucket run keep their keys apart
  (openspec/changes/stage-5-5-three-role-refactor/specs/quad-storage/spec.md, "Every key belongs to one region", verbatim).

  Scenario: Two tenants sharing a run keep their keys apart
    When tenants 7 and 8 share a bucket run, and each writes `<a> <p> <b>`, whose subject takes one bucket in both
    Then each statement's keys hold its own tenant in bytes 1 to 8, a read of tenant 7 returns none of tenant 8's, and the partition owning the bucket holds both
