## ADDED Requirements

### Requirement: Queries fan out to the serving groups
Each predicate of a query SHALL be evaluated by a group that serves it, after that group has applied every commit at
or below the query's timestamp, and the result SHALL equal the result over one group.

#### Scenario: Two groups answer as one
- **WHEN** Dgraph's test data is loaded into two groups with its predicates split between them and the query suites run
- **THEN** every result equals the result over one group

### Requirement: Requests to a group are hedged
A task or sort request sent to another group SHALL go to one of two of its replicas and, without a reply within one
second, also to the other; an error from the first before then SHALL retry on the second.

#### Scenario: A down replica does not fail the query
- **WHEN** one replica of the serving group is stopped and a query reads its predicate
- **THEN** the query succeeds through another replica
