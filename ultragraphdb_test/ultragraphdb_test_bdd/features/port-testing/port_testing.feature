#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Port testing harness
  Spec scenarios are traced to feature scenarios, feature suites are declared for both build systems, and
  byte-exact behaviour is checked against golden vectors from the Go source.

  Scenario: An archived scenario without a feature fails
    Given an archived spec declaring the scenario "Lone archived scenario"
    And no feature naming it
    When the traceability test runs
    Then it fails naming "Lone archived scenario" and its spec file

  Scenario: A feature scenario without a spec fails
    Given a feature naming the scenario "Orphan feature scenario"
    And no spec declaring it
    When the traceability test runs
    Then it fails naming "Orphan feature scenario" and its feature file

  Scenario: A change's scenarios without features are pending
    Given the change "stage-x" declaring the scenario "Not built yet"
    And no feature naming it
    When the traceability test runs
    Then it passes and lists "Not built yet" as pending in "stage-x"

  Scenario: Scenario names are unique
    Given two specs declaring the scenario "Twice declared"
    When the traceability test runs
    Then it fails naming "Twice declared" and both spec files

  Scenario: A crate with features declares both suites
    When the declaration test reads each crate holding a features folder
    Then each declares the Cargo test and the Bazel target as required

  Scenario: The test script runs the BDD suites
    When the declaration test reads the test script
    Then it finds a Bazel test run filtered on bdd-test after the run filtered on unit-test

  Scenario: Every golden file names its Dgraph commit
    When the golden files of the workspace are read
    Then each records the Dgraph commit it was generated from

  Scenario: A golden mismatch names the input
    Given the golden bytes "00 01" for the input "x"
    When the output "00 02" is compared with them
    Then the comparison fails and reports "x", "00 01" and "00 02"
