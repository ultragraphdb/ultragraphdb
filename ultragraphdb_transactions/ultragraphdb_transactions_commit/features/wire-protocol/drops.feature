#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: drops are commands
  A client drops a predicate, a graph, a namespace's data, a namespace or every namespace by a Drop request on a stream
  of its own, committed through a session as a commit is and answered with the drop's commit version. The commit step
  decides a drop as the last transaction of its round, and its write-set record names the drop, so recovery redoes it.
  A transaction that read or wrote what a drop removes, below the drop's version, aborts with a conflict of kind Drop.
  Here the drop is the commit step's proposal on a changeset applier over the simulated store; the Drop command on the
  wire is task 10.1's.

  Scenario: A graph drop removes the graph
    When a client drops graph `g` of a namespace holding statements in `g` and in `h`
    Then the reply carries the drop's commit version, no statement of `g` remains, and every statement of `h` remains

  Scenario: A namespace's data drop keeps its catalog
    When a client drops a namespace's data after declaring `name` a property
    Then no statement of the namespace remains, and Describe still lists `name`

  Scenario: A drop retried through its session applies once
    When a Drop's reply is lost, and the client sends it again with the same session and request number
    Then the second answer is the stored reply, and the drop is applied once

  Scenario: A transaction writing into a dropped graph aborts
    When a transaction reads before a drop of graph `g` commits and then writes into `g`
    Then its commit is refused with a conflict of kind Drop
