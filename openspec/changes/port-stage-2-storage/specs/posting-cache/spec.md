## ADDED Requirements

### Requirement: The cache never returns a stale list
A posting list read through the cache at timestamp `t` SHALL equal the same list read from the store at `t`.

#### Scenario: A read after a commit sees the commit
- **WHEN** a list is cached, a commit at 20 changes it, and it is read at 21
- **THEN** the read reflects the commit at 20

#### Scenario: Cached and uncached reads agree under concurrent commits
- **WHEN** commits and reads at increasing timestamps run concurrently against the same keys with the cache on
  and with it off
- **THEN** every read returns the same result in both runs

### Requirement: The cache changes with the write it reflects
A commit SHALL update or remove the cache entries of the lists it wrote in the same critical section as the write
batch, and a rollup SHALL replace or remove the entry of the list it rolled up.

#### Scenario: No reader sees the batch without the cache update
- **WHEN** a commit writes a list that is cached
- **THEN** no read that sees the batch in the store returns the cached list from before the batch

### Requirement: Schema changes and drops clear the cache
A schema change, an index rebuild and any drop SHALL clear the cache.

#### Scenario: A drop clears the cache
- **WHEN** a cached predicate is dropped
- **THEN** a read of that predicate finds nothing
