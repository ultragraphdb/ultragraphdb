## ADDED Requirements

### Requirement: Zero assigns a tablet on first use
The first mutation or schema update naming a predicate, or an Alpha starting with it in its schema, SHALL make Zero
assign its tablet to the group of the Alpha handling it, unless its name begins with `dgraph.` (ignoring case), which
SHALL always go to group 1. When two groups claim one predicate at once, exactly one SHALL own it and the other SHALL
learn the owner, `Inform` answering with the owner's tablet where Dgraph answers with the caller's (`zero.go:428`).

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
A group SHALL refuse to apply a write for a predicate it does not serve, with `Tablet isn't being served by this
instance`, and SHALL serve a read only after its membership checksum equals Zero's and only for a predicate it serves.

#### Scenario: A write for another group's predicate is refused at apply
- **WHEN** a proposal for `color` reaches group 1 while group 2 serves `color`
- **THEN** group 1 refuses it as an unserved tablet

### Requirement: Tablet sizes are reported
Each group's leader SHALL report its tablets' on-disk sizes to Zero every 5 minutes, summed per predicate by the store.
Zero SHALL record a size that differs from the recorded one by more than 10%, or is non-zero where the recorded one is
zero, and SHALL instruct the group to delete any reported tablet it no longer serves unless a move is running.

#### Scenario: A grown tablet's size reaches Zero
- **WHEN** a tablet grows by 50% and the next report runs
- **THEN** Zero's state shows the new size

#### Scenario: A group drops a tablet it no longer serves
- **WHEN** a group reports a tablet Zero assigns to another group and no move is running
- **THEN** Zero instructs the group to delete it, and the group's store no longer holds it
