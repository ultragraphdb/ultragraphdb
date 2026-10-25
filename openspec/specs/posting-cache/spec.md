# posting-cache Specification

## Purpose
The cache of posting lists between reads and the store: it never returns a stale list, it changes with the write
it reflects, a reader fills it only with a current list, and schema changes and drops clear it.
## Requirements
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

### Requirement: A reader fills the cache only with a current list
A reader that missed the cache SHALL insert the list it read from the store only if no commit to that key was applied
after the reader's snapshot and the cache's epoch is the one the reader started under, checked under the writer lock.

#### Scenario: A stale fill is refused
- **WHEN** a reader misses, a commit then writes the key, and the reader then inserts its list
- **THEN** the insert is refused and a read at the commit's timestamp reflects the commit

### Requirement: Schema changes and drops clear the cache
A schema change, an index rebuild and any drop SHALL advance the cache's epoch under the writer lock, after which an
entry of an older epoch SHALL be a miss and a fill begun under an older epoch SHALL be refused.

#### Scenario: A drop clears the cache
- **WHEN** a cached predicate is dropped
- **THEN** a read of that predicate finds nothing

#### Scenario: A fill begun before a drop is refused
- **WHEN** a reader misses a list, the list's predicate is then dropped, and the reader inserts its list
- **THEN** the insert is refused, and a read of the predicate finds nothing

