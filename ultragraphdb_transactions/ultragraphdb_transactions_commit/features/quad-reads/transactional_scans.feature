# The scenarios of "A read through compute is a read of its transaction" at the transaction's level, from
# openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md: a transaction scanning as a Scan naming it does,
# decided by the commit step. Reaching it through compute and the frontend's ordering of a read and a write are 9.5.3 to 9.5.5.

Feature: A read through compute is a read of its transaction

  Scenario: A read sees its transaction's own write
    When a transaction writes `<a> <knows> <b>`, and the same transaction reads `<a> <knows> ?o` before committing
    Then the read returns `<b>`, and a read by any other transaction does not

  Scenario: A read sees its transaction's new term and its emptied list
    When a transaction writes `<new> <p> "x"`, creating `<new>`, removes every object of `<a> <p> ?o`, and reads both
    Then the read returns `"x"` for `<new>` with `<new>` named by its text, and nothing for `<a>`

  Scenario: A scan through compute holds no list in the transaction
    When a transaction scans 10,000 lists through compute
    Then its read set holds one range and no list key, and it holds none of those lists in memory afterwards

  Scenario: A scan read in several frames holds one range and conflicts as one scan does
    When T1 reads a range of two subjects' lists through compute one object a frame, T2 commits an insert into a list it read, and T1 commits a write
    Then its read set holds one range and no list key, and T1 aborts with a conflict of kind Range

  Scenario: A term nobody holds conflicts with its later statement through compute
    When transaction T1 reads `<nobody> <p> ?o` through compute, finding nothing, transaction T2 commits `<nobody> <p> "x"`, and T1 commits a write
    Then T1 aborts with a conflict of kind AbsentTerm

  Scenario: A term no read can hold records nothing
    When a transaction resolves an unheld skolem IRI
    Then it answers no ID, and its read set records no absent term

  Scenario: A scanned range conflicts with an insert into it through compute
    When transaction T1 reads `<a> <knows> ?o` through compute, finding nothing, transaction T2 commits `<a> <knows> <b>`, and T1 commits a write
    Then T1 aborts with a conflict of kind Range

  Scenario: A reverse range conflicts with an edge whose reverse entry lands in it
    When transaction T1 reads `?s <knows> <b>` through compute, transaction T2 commits `<c> <knows> <b>` in the same partition, and T1 commits a write
    Then T1 aborts with a conflict of kind Range

  Scenario: An index range conflicts with a write whose index entry lands in it
    When transaction T1 scans a declared predicate's value index through compute, transaction T2 commits a value of it, and T1 commits a write
    Then T1 aborts with a conflict of kind Range

  Scenario: A range stopped at its limit conflicts only with what it returned
    When T1 reads with limit 1 a range holding `<a>` and `<z>`, T2 commits an insert past `<a>` that T1's read did not reach, and T1 commits without resuming
    Then T1 commits

  Scenario: Resuming records the rest
    When T1 reads with limit 1, resumes to the range's end, T2 commits an insert into the resumed part, and T1 commits a write
    Then T1 aborts with a conflict of kind Range

  Scenario: A read past the read set's room ends its transaction
    When a transaction's reads leave no room for a span
    Then the read is refused with `TooLarge`, no entry of that span reaches the client, and the transaction's next request is refused

  Scenario: A statement's record read conflicts with a property written
    When T1 reads the properties of the reifier `r` through compute, T2 sets a property through `r` and commits, and T1 commits a write
    Then T1 aborts with a conflict of kind Range
