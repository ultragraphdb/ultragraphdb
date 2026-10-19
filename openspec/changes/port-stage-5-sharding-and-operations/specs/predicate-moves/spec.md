## ADDED Requirements

### Requirement: A move preserves the predicate
Moving a predicate SHALL copy every value, language value, facet, index, reverse and count entry to the
destination at the move timestamp, replace any stale copy there, reassign the tablet, and then remove the predicate
from the source.

#### Scenario: Every entry arrives
- **WHEN** an indexed, reversed, counted predicate with facets and language values is moved
- **THEN** every query over it returns the same result after the move as before

#### Scenario: The source no longer holds the predicate
- **WHEN** a move completes
- **THEN** the source group's store holds no key of the predicate

#### Scenario: Stale data at the destination is replaced
- **WHEN** the destination holds an old copy of the predicate from an earlier failed move
- **THEN** after the move it holds exactly the source's copy

### Requirement: Commits on a moving predicate abort
While a predicate moves, commits writing it SHALL abort and commits not touching it SHALL proceed; reads SHALL
continue, and a read below the move timestamp after the move SHALL fail with Dgraph's error.

#### Scenario: Writes to the moving predicate abort and others commit
- **WHEN** a move of `color` is running and transactions write `color` and `shape`
- **THEN** the `color` writes abort and the `shape` writes commit

#### Scenario: A read below the move timestamp fails after the move
- **WHEN** a query with a start timestamp below the move timestamp reads the moved predicate after the move
- **THEN** it fails naming the move timestamp

### Requirement: Moves are checked and run one at a time
A move of a reserved predicate, to an unknown group, to the current owner, or of a predicate no group serves SHALL
be refused; one move SHALL run at a time.

#### Scenario: Moving a reserved predicate is refused
- **WHEN** a move of `dgraph.type` is requested
- **THEN** it is refused
