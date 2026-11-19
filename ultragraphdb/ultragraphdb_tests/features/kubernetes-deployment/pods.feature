# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The deployment's pods as processes
  openspec/changes/stage-5-8-system-group/specs/kubernetes-deployment/spec.md: the scenarios of its four requirements that a
  running cluster decides, bound over the built binaries as task 8.2 plans. A pod is a process started through the config
  service; a pod deleted with a grace period of zero is a process killed by SIGKILL; a StatefulSet or a Deployment starting it
  again is the same instance started again on the settings the config service holds, a StatefulSet's over the same data
  directories, as its volume holds them. What the rendered chart decides is `ultragraphdb_chart_tests`'. Compute's part
  follows openspec/changes/stage-5-9-compute-role/specs/kubernetes-deployment/spec.md; "An empty namespace comes up" leaves
  out the node list a client receives, which no binary sends before the stage's group 7 binds that clause.

  Scenario: An empty namespace comes up
    When the chart is installed into an empty namespace of a cluster holding the images
    Then the config service, every storage instance and every compute pod each answer `/ready`, no compute pod mounts a volume, and a client reaching compute's client Service authenticates and commits

  Scenario: A storage pod restarts at its address
    When a storage pod is deleted and its StatefulSet starts it again with another IP
    Then it joins the system group from the same role address, claims the same partitions, and compute's next request to it is answered

  Scenario: A missing role secret stops a role
    When the role secret's Secret is absent and a storage pod starts
    Then the pod exits with code 78 naming the role secret's file, and starts once the Secret exists

  Scenario: A storage pod killed under writes keeps every acknowledged commit
    When a storage pod is deleted with a grace period of zero while a client commits through compute, and the pod starts again
    Then every commit acknowledged before the deletion reads back once the pod is ready

  Scenario: The hosting storage pod killed keeps the books
    When the storage pod hosting the system group is deleted with a grace period of zero after a session was registered and a catalog version committed, and the pod starts again
    Then the session's next commit is accepted and the catalog version is still the tenant's

  Scenario: A deleted compute pod loses nothing
    When a compute pod is deleted and its StatefulSet starts another
    Then a client reconnects, authenticates, and commits through the session it registered before
