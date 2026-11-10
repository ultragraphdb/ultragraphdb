#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: the frames of stage 6
  openspec/changes/stage-6-ugs-data-definition/specs/wire-protocol/spec.md: Describe's continuation frames, Install's
  warnings, Drop's catalog version, the reifier column's handle IRIs, the golden frames of the bodies stage 6 changes,
  UQL as a Query's language 3, and Operation kinds 5 and 6. These are the scenarios whose decision this crate makes:
  each frame's bytes, read and built. The frontend sends and refuses them in its own features; Purge and Migrate's
  streams and a Describe's credit run in the transport's.

  Scenario: A Describe past the client's frame continues
    When a tenant's Describe answer does not fit one frame within the client's `max_frame`
    Then it arrives in continuation frames whose sections concatenate to the whole answer, the last flagged `LAST`

  Scenario: An Install reply carries its warnings
    When an Install reply with one warning `UGS0902` is built and read back
    Then the view answers the version, the epoch, the report and one warning whose `diagnostic` is 902 in language 4, with its source, location and message

  Scenario: A request with a report is refused
    When an Install request's section 2 is not empty
    Then it is refused with `CountMismatch` naming `report`

  Scenario: A Drop of a predicate names no catalog version
    When a Drop of kind 1 carries `catalog` 7
    Then it is refused with `FieldValue` naming `catalog`

  Scenario: A handle IRI is taken in the reifier column
    When a pattern's Result batch holds, in its reifier column, a term of tag `0x01` and another of tag `0x30`
    Then the first batch is read, and the second is refused with `FieldValue` naming `reifier`

  Scenario: The golden frames match their generators
    When the conformance suite reads every golden frame of Result, Prepare, Describe, Install and Reference
    Then each decodes, and building it again from its view gives the same bytes

  Scenario: Language 3 is read and built
    When a Query with `language` 3, the text `Company { id }` and no parameters is built and read back
    Then the view answers language 3 and the text

  Scenario: Language 4 is refused
    When a Query names `language` 4
    Then it is refused with `FieldValue` naming `language`

  Scenario: A retract operation is read and built
    When an Operation of kind 6 naming the default graph and `https://example.org/deep_brain#Node/ORG%3Aacme` is built and read back
    Then the view answers kind 6, the graph `ABSENT` and the IRI

  Scenario: A delete operation with a third arg is refused
    When an Operation of kind 5 carries three args
    Then it is refused with `CountMismatch` naming `args`
