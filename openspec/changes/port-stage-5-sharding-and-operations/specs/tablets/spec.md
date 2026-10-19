## ADDED Requirements

### Requirement: Zero assigns a tablet on first write
The first write to a predicate SHALL make Zero assign its tablet to the writing group, unless the predicate is
reserved, which SHALL always go to group 1; when two groups claim one predicate at once, exactly one SHALL own it and
the other SHALL learn the owner.

#### Scenario: A first write assigns the tablet to the writer's group
- **WHEN** group 2 writes the first value of `color`
- **THEN** Zero lists `color` under group 2, and a later write from group 1 is routed to group 2

#### Scenario: Two groups racing for a predicate end with one owner
- **WHEN** groups 1 and 2 write the first value of `shape` at the same moment
- **THEN** exactly one group serves `shape` and both writes land there

#### Scenario: Reserved predicates live in group 1
- **WHEN** group 2 writes `dgraph.type`
- **THEN** the write lands in group 1

#### Scenario: A read of an unknown predicate creates no tablet
- **WHEN** a query reads `unknown` that no group serves
- **THEN** the query returns nothing for it and Zero lists no tablet for `unknown`

### Requirement: Ownership is checked where data is touched
A group SHALL refuse to apply a write or serve a read for a predicate it does not serve, once its membership agrees
with Zero's.

#### Scenario: A write for another group's predicate is refused at apply
- **WHEN** a proposal for `color` reaches group 1 while group 2 serves `color`
- **THEN** group 1 refuses it as an unserved tablet

### Requirement: Tablet sizes are reported
Each group's leader SHALL report its tablets' sizes to Zero every 5 minutes, measured from the store, and Zero SHALL
record a size change above 10%.

#### Scenario: A grown tablet's size reaches Zero
- **WHEN** a tablet grows by 50% and the next report runs
- **THEN** Zero's state shows the new size
