#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The catalog's declarations
  A predicate declares its kind, cardinality, datatype constraint, unique values and indexes; an entity type has at
  most one parent and a number in a preorder walk of the hierarchy.

  Scenario: A tokenizer that cannot take the datatype constraint is refused
    When age is declared with datatype xsd:string and an int index
    Then the declaration fails with an error naming age, int and xsd:string

  Scenario: A second parent is refused
    When Employee is declared a subtype of Person and then of Agent
    Then the second declaration fails with an error naming Employee, Person and Agent

  Scenario: Preorder numbers follow the hierarchy
    When Agent has the subtypes Person and Org, and Person the subtype Employee
    Then the preorder numbers place Employee between Person and Org
