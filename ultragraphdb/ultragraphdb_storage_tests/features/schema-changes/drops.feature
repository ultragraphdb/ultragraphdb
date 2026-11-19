# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@drops
Feature: Drops through compute, read back at the partition
  openspec/changes/stage-5-8-system-group/specs/schema-changes/spec.md, "Drops": the frontend and a storage process hosting
  the system group as processes, each Drop sent by a client through the frontend, and the partition's keys read back over its
  role address with a Scan of the tenant's key ranges. "Dropping a vector predicate drops its vector index" is not bound
  here: the frontend refuses a vector index until stage 8 (client-endpoint, "A vector index is refused"). "Dropping a tenant's
  everything survives a restart" and "A tenant's drop leaves a tenant sharing its run" are not bound here: tenants 3, 7 and 8
  cannot exist in this stage, which holds tenant 0 alone (metadata-group, "The metadata group holds the tenants").

  Scenario: Dropping a predicate removes it from the predicate lists
    When `knows` is held for `a` and `b` and is dropped
    Then no key of `knows` remains, and neither the subject predicates of `a` nor of `b` holds `knows`

  Scenario: Dropping a graph keeps the other graphs
    When graphs `<g1>` and `<g2>` hold statements and `<g1>` is dropped
    Then no key of `<g1>` remains, and every statement of `<g2>` is still held
