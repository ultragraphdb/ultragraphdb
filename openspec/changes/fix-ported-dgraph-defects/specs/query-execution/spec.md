## ADDED Requirements

### Requirement: Pagination past a list's end returns nothing
A filtered read with `after:` past the last UID of a list SHALL return no UIDs from that list. Dgraph ends the Alpha
process on it, so the differential runs against Go Dgraph exclude it (ledger 3).

#### Scenario: after past the last UID under a filter returns nothing
- **WHEN** a query's filter intersects a rolled-up list and `after:` lies past the list's last UID
- **THEN** the query returns no UIDs from that list and the server keeps serving
