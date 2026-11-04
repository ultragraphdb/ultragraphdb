# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A target's settings are immutable numbered revisions
  openspec/changes/stage-5-5-three-role-refactor/specs/config-service/spec.md

  Scenario: A rollback activates an older revision
    When target `ci` holds revisions 1 and 2, with 2 active, and an administrator activates 1
    Then a role starting next receives revision 1, and revision 2 is still stored

  Scenario: A revision edited from a stale one is refused
    When two administrators edit from revision 3, and the first adds revision 4
    Then the second's revision is refused naming revision 4, and nothing is stored

  Scenario: A restart holds what was acknowledged
    When a revision is acknowledged and the service is killed and restarted
    Then the revision is stored, and the active revision is the one last acknowledged

  Scenario: A torn record is not a revision
    When the service is killed while appending a revision it has not acknowledged
    Then after a restart the revision is absent or whole, never partial

  Scenario: A central file begins a target once
    When a service starts declaring target `ci`, and again after a declared file changed validly
    Then `ci` holds revision 1 alone, and the second start reports that the file differs and the revision still active

  Scenario: An active revision a release no longer accepts stops the start
    When a store's active revision names a setting the release does not hold
    Then the start's check names the target, the revision and the setting

  Scenario: A maximal revision fits one round
    When a revision's record holds the most a request can
    Then the log takes it in one round

  Scenario: A retired target serves nothing, and an activation ends it
    When a target is retired
    Then asking for an instance says it is retired, its revisions are still stored, and activating one serves it again
