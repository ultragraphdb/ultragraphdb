## MODIFIED Requirements

### Requirement: Count indexes
For a predicate the catalog declares an exact count index on, a subject SHALL be listed under the forward count key of
its current number of objects when that number is above zero and under no count key otherwise, a single-valued
predicate counting as one. An object SHALL be listed, in each partition, under the reverse count key of the number of
that partition's own subjects pointing at it (partitions, "A partition owns every entry its subjects' writes derive"),
so its exact number of subjects is the sum of the counts of the partitions its referrer set names, and a lookup by
reverse count, compute's own read for the query core, called through compute's API since no client request names one in
this stage, SHALL ask every partition of the tenant and sum each object's counts before it compares them. A write
that adds no object SHALL leave the counts unchanged.

#### Scenario: Adding a third object moves the count
- **WHEN** `friend` has an exact count index, `a` holds two objects of it, and a third is written
- **THEN** `a` is listed under forward count 3 and not under 2

#### Scenario: Re-writing an existing object leaves the count
- **WHEN** an object `a` already holds is written again
- **THEN** the count keys of `a` do not change

#### Scenario: Reverse counts are kept
- **WHEN** `friend` has an exact count index and gains `<a> <friend> <b>`
- **THEN** the reverse count keys of `b` in `<a>`'s partition reflect one more subject

#### Scenario: A reverse count sums the referrers' partitions
- **WHEN** `friend` has an exact count index, two subjects in partition 1 and one in partition 2 hold `friend` to `<b>`,
  and compute's API looks up the objects of reverse count 3
- **THEN** partition 1 lists `<b>` under reverse count 2 and partition 2 under 1, and the lookup returns `<b>`
