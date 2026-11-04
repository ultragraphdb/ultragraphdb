Feature: Term dictionary: registration at an entity's home

  The term-dictionary scenarios of registration, verbatim from the change's specs/term-dictionary/spec.md.

  Rule: A reference to another partition's entity registers at its home

    Scenario: A first reference registers and waits
      When a transaction in partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2 and new
      Then partition 2 creates `<b>`'s ID and adds `<a>`'s bucket group to `R(b)` durably before it replies, and partition 1 commits only after the reply, writing the mark of `(<b>, <a>'s group)` and `<b>`'s dictionary entries with `<a>`'s statement

    Scenario: A marked reference sends no registration
      When partition 1 has committed a reference to `<b>` from a subject of bucket group 7, and a later transaction writes another edge to `<b>` from a subject of group 7
      Then it finds `<b>`'s ID and the mark in its own store, sends no registration, and commits without waiting for partition 2, after a restart too

    Scenario: A second group of the same partition registers
      When partition 1 holds the mark of `(<b>, 7)`, and a transaction writes an edge to `<b>` from a subject of partition 1's bucket group 9
      Then it registers `(<b>, 9)` at partition 2, `R(b)` names groups 7 and 9, and partition 1 then holds both marks

    Scenario: A foreign predicate is got without joining a referrer set
      When partition 1 first writes a statement whose predicate `<knows>` is homed in partition 2
      Then partition 1 gets `<knows>`'s ID from partition 2 before the commit and copies its entries in both directions, and no referrer set gains a member for `<knows>`

    Scenario: Registrations to one node are batched
      When a transaction in partition 1 writes edges to 50 new IRIs in one write request, all homed in partitions of another storage process
      Then partition 1 sends that process one batch of 50 registrations for the request

    Scenario: Concurrent registrants share one ID
      When partitions 1 and 3 each register a reference to the new IRI `<b>` at its home, both registrations reaching the home before it replies to either
      Then both replies name the same ID, both partitions' transactions commit, and `R(b)` names both groups
