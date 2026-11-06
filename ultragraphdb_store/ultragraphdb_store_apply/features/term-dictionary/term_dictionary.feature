#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The term dictionary under changesets
  A statement ID's reifier is a skolem IRI the dictionary maps to the ID and back, and every writer of a new IRI takes
  its one ID while executing, none aborting for it (openspec/changes/stage-5-5-three-role-refactor/specs/
  term-dictionary/spec.md): the scenarios a changeset's execution decides, verbatim.

  Scenario: A statement ID's reifier is a skolem IRI
    When `<a> <knows> <b>` gains the property `<since> 2020` and so receives a statement ID
    Then the dictionary holds a skolem IRI `…/tenant/{n}/.well-known/genid/{bucket}-{random}` for it, whose bucket is `<a>`'s, mapping to the statement ID and back, and a write naming that IRI as the reifier adds properties to the statement

  Scenario: Two creators share one ID
    When two transactions in `<new>`'s home read before either commits, both write `<new> <p> "x"`, not held, and both send their commit
    Then both receive one system ID for `<new>` and neither aborts for creating it; the two still conflict on the statement (transactions, "What a transaction's reads record and what conflicts with them"), so exactly one commits, and `<new>` has one system ID
