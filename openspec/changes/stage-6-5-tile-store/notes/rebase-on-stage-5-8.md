# Rebase on stage 5.8

Stage 5.8's change (`openspec/changes/archive/2026-11-17-stage-5-8-system-group`) synced its deltas into
`openspec/specs/` on 2026-11-18. Archiving copies a MODIFIED requirement's whole text from this change's delta, so each
requirement below, which this change modifies and stage 5.8 changed, is brought to the archived text first. Then only
this change's own edits differ from it (task 0.7). Every stage 5.8 scenario is kept unless this change
removes it on purpose, and each such removal is stated in the delta.

8 requirements are modified by both. Under each, the scenarios of stage 5.8's archived text that this
change's delta does not hold, compared on 2026-11-17:

- `partitions`, "A storage process hosts partitions over bucket ranges":
  - "The system group forms the placement"
- `role-binaries`, "Each role is its own binary and image":
  - "The frontend binary carries no book"
- `role-binaries`, "Each role runs one runtime per core": every scenario held; the requirement's text is compared
- `role-binaries`, "Readiness, health and shutdown":
  - "The frontend is ready only once the books answer"
  - "The frontend is unready while its system group is unreachable"
- `schema-changes`, "Drops":
  - "Dropping a predicate removes it from the predicate lists"
- `transactions`, "Exactly-once commits through sessions":
  - "A partition purges an entry only after the registry's eviction"
  - "A retry checked in the quiet time stays undecided"
- `wire-protocol`, "Data requests, prepared statements and describe":
  - "Describe names the catalog version it is drawn from"
  - "A client's Describe naming a catalog version is refused"
  - "A Describe continues across frames"
  - "A Describe past the catalog's bound is refused"
- `wire-protocol`, "Roles exchange role commands":
  - "A list larger than a frame continues across frames"
  - "A Scan's `after` is read from the frame"

## What the rebase must settle

Compared on 2026-11-18, scenario by scenario, against `openspec/specs/` as stage 5.8 archived it. Stage 5.9 changes nine
of the same capabilities again (the role's name, the removed relay, the node list), so this change is rebased once,
after stage 5.9 is archived, and the comparison is repeated then.

### 1. Deltas that would undo stage 5.8

Archiving copies a MODIFIED requirement whole, so each stale copy below would replace stage 5.8's text.

| Requirement | What the stale delta does |
| --- | --- |
| `partitions`, "A storage process hosts partitions over bucket ranges" | Holds "The frontend forms the placement"; stage 5.8 made it "The system group forms the placement" |
| `transactions`, "Exactly-once commits through sessions" | Drops "A retry checked in the quiet time stays undecided" |
| `wire-protocol`, "Data requests, prepared statements and describe" | Drops "Describe names the catalog version it is drawn from", "A client's Describe naming a catalog version is refused", "A Describe continues across frames" and "A Describe past the catalog's bound is refused" |
| `schema-changes`, "Drops" | Holds "Dropping a predicate holding vectors drops its values" and "A tenant's drop leaves a tenant sharing its run", which stage 5.8 moved to the backlog (items 22 and 21, stages 8 and 7) |
| `role-binaries`, "Each role is its own binary and image" and "Readiness, health and shutdown" | Drop stage 5.8's scenarios of the frontend's books and readiness; stage 5.9 restates them for compute |

### 2. Defects of this change's own

- **A contradiction with stage 6.6 and with the code.** This change's `wire-protocol` delta says "A Describe past the
  client's frame is refused". Stage 6.6's says it "continues", and stage 5.8 built "A Describe continues across frames".
- **A MODIFIED requirement the main specifications do not hold.** `openspec validate` passes, and `openspec archive`
  fails on it:
- `tokenizers`, "Geo tokens index S2 cells"

### 3. Differences that are this change's content

The other differences are the tile store itself: lists become rows, the KV store's read kernel becomes ranges and
families, checkpoints and the simulator's faults change. The rebase keeps them and still compares each requirement's
text. Every MODIFIED requirement whose scenarios differ from the main specifications:

- `commit-rounds`, "Catalog changes, drops and index builds are ordered in rounds"
  - drops: "A crash during a build runs the backfill again"
  - adds: "A crash during a build resumes from its record"
  - adds: "A drop is the last record of its round"
- `commit-rounds`, "Commit versions are built from the epoch"
  - adds: "A round with no committed entry consumes a version"
- `local-commit`, "A request reads its own earlier edges"
  - drops: "A list first written blind is read whole"
  - adds: "A row first written blind is read whole"
- `local-commit`, "Applied timestamps gate reads"
  - drops: "A read waits for a commit below its timestamp"
  - drops: "A rollup leaves an open read intact"
  - adds: "A read above the watermark answers by its epoch"
  - adds: "A rewrite leaves an open read intact"
- `partitions`, "A storage process hosts partitions over bucket ranges"
  - drops: "The system group forms the placement"
  - adds: "The frontend forms the placement"
  - adds: "Each partition has a grid of its own"
- `quad-reads`, "A read through compute is a read of its transaction"
  - drops: "A read sees its transaction's new term and its emptied list"
  - drops: "A scan through compute holds no list in the transaction"
  - drops: "A scanned range conflicts with an insert into it through compute"
  - drops: "A reverse range conflicts with an edge whose reverse entry lands in it"
  - drops: "A range stopped at its limit conflicts only with what it returned"
  - drops: "A limit stopping before a graph leaves that graph unread"
  - drops: "Resuming records the rest"
  - drops: "A span recorded after a Cancel only widens the commit's check"
  - adds: "A read sees its transaction's new term and its emptied row"
  - adds: "A family read whole holds one range in the transaction"
  - adds: "A row read conflicts with an insert into it through compute"
  - adds: "A reverse row conflicts with an edge whose reverse entry lands in it"
  - adds: "A family read whole conflicts past its limit"
  - adds: "What a request records after a Cancel only widens the commit's check"
- `quad-reads`, "Access paths follow the bound positions"
  - drops: "A bound object reads the reverse keyspace"
  - drops: "An unbound graph is walked by Scans of limit 1"
  - drops: "A graph appearing in a gap conflicts"
  - adds: "A bound object reads the reverse rows"
  - adds: "A bound subject alone reads every predicate"
  - adds: "An unbound graph with a bound subject is read by a Frontier naming every graph"
  - adds: "A graph appearing later conflicts"
- `quad-reads`, "Compute executes quad-pattern reads over storage"
  - drops: "An unbound subject merges every partition"
  - drops: "A vector object reads the forward lists"
  - drops: "A term absent where its list lies answers no row"
  - drops: "A term and its list take two Scans"
  - drops: "A list Scan waits for every partition's close"
  - adds: "An unbound subject answers partition by partition"
  - adds: "A vector object reads the forward families"
  - adds: "A term absent where its rows lie answers no row"
  - adds: "A term and its rows take two reads"
  - adds: "A snapshot read's Frontier names its Scan's version"
  - adds: "A read of rows waits for every partition's close"
- `quad-reads`, "Resume keys continue their own read"
  - drops: "A resumed version no longer retained is refused"
  - adds: "A resumed version below the fold horizon is refused"
  - adds: "A resumed read continues in its partition"
- `quad-reads`, "Storage's read kernel answers what compute scans"
  - drops: "A dictionary and a referrer set are read by Scan"
  - drops: "A hub's list crosses frames"
  - drops: "A keyspace outside the kernel's is refused"
  - drops: "A range of the dictionary holds one key"
  - drops: "A Scan of a partition the address does not serve is refused"
  - drops: "A record too large for a frame is refused"
  - drops: "A large list's frames stay within a turn"
  - drops: "A range of emptied lists takes many turns"
  - drops: "A Scan of a transaction ends with it"
  - drops: "A Scan's ranges name one tenant"
  - drops: "A resumed range reads its list from `after`"
  - drops: "A malformed `after` is refused"
  - adds: "A dictionary record and a referrer set are read by Scan"
  - adds: "A hub's row crosses frames"
  - adds: "A record kind outside the kernel's is refused"
  - adds: "A request of a partition the address does not serve is refused"
  - adds: "A record larger than a frame continues"
  - adds: "A large row's frames stay within a turn"
  - adds: "A family of emptied rows takes many turns"
  - adds: "A request of a transaction ends with it"
  - adds: "A resumed range answers strictly after its resume key"
  - adds: "A malformed resume key is refused"
  - adds: "A Frontier answers its rows in the request's order"
- `quad-storage`, "A fresh term's lists are not read"
  - drops: "Linking a new subject reads none of its lists"
  - adds: "Linking a new subject reads none of its rows"
- `quad-storage`, "Statement IDs"
  - drops: "Replay derives both postings"
  - adds: "Replay derives both entries"
- `quad-storage`, "Statistics are kept by deltas"
  - adds: "Apply writes the round's change as carried"
- `role-binaries`, "Each role is its own binary and image"
  - drops: "The frontend binary carries no book"
- `role-binaries`, "Each role runs one runtime per core"
  - adds: "Storage's store runs on its partitions' threads"
- `role-binaries`, "Readiness, health and shutdown"
  - drops: "The frontend is ready only once the books answer"
  - drops: "The frontend is unready while its system group is unreachable"
- `schema-changes`, "Adding an index rebuilds it from existing data"
  - adds: "A build interrupted by a crash resumes where its record stands"
- `schema-changes`, "Drops"
  - drops: "Dropping a predicate removes it from the predicate lists"
  - adds: "Dropping a predicate leaves none of its entries"
  - adds: "Dropping a predicate holding vectors drops its values"
  - adds: "Dropping a tenant's everything survives a restart"
  - adds: "A tenant's drop leaves a tenant sharing its run"
  - adds: "Dropping everything removes the default entries"
- `schema-changes`, "Removing an index drops only its entries"
  - adds: "A tokenizer removed without a build leaves under a drop marker"
- `storage-simulation`, "Invariants hold after every step"
  - adds: "The dictionary workload passes on the simulator"
  - adds: "Two replicas agree under faults"
- `storage-simulation`, "The simulator injects storage faults"
  - drops: "A power loss keeps every synced batch"
  - drops: "A failed batch poisons the store"
  - drops: "A drop interrupted by a power loss finishes at recovery"
  - drops: "A corrupted value is refused"
  - adds: "A power loss keeps every acknowledged round"
  - adds: "A failed sync trusts nothing it covered"
  - adds: "A refused punch leaves the bytes and frees the extent"
  - adds: "A misdirected read is not answered"
- `storage-simulation`, "The store is reached through one trait"
  - drops: "The posting lists run on the simulated store"
  - adds: "The log and the grid run on the simulated storage"
- `tokenizers`, "Sortable tokens preserve order"
  - drops: "Float tokens truncate toward zero"
  - drops: "Hour tokens hold UTC fields"
  - adds: "Float tokens are exact"
  - adds: "A generic literal's datatype picks its token"
  - adds: "Values of two sortable types never share a token"
  - adds: "A float is parsed as a double, and a decimal past 18 fraction digits takes no token"
  - adds: "A count of zero takes its token"
  - adds: "A count token holds its direction"
  - adds: "A long token is hashed"
- `tokenizers`, "Text tokenizers follow Dgraph's analysis"
  - drops: "Stemmed languages match Dgraph per language"
  - adds: "Stemmed languages stem per language"
- `tokenizers`, "Tokenizer identifiers"
  - drops: "Every tokenizer's tokens match the golden vectors"
  - adds: "Every tokenizer's identifier byte is its own"
- `tokenizers`, "Tokenizer registry"
  - drops: "hnsw options are validated"
  - adds: "The registry holds no vector index factory"
- `transactions`, "Aborts name their reason"
  - adds: "A full delta answers busy with its detail"
- `transactions`, "Exactly-once commits through sessions"
  - drops: "A partition purges an entry only after the registry's eviction"
  - drops: "A retry checked in the quiet time stays undecided"
  - adds: "A partition deletes an entry at the round logging its eviction"
- `transactions`, "What a transaction's reads record and what conflicts with them"
  - drops: "Inserts of different objects into one list both commit"
  - adds: "Inserts of different objects into one row both commit"
  - adds: "A family read whole conflicts with a new row"
  - adds: "A write into a dropped range conflicts"
- `wire-protocol`, "Data requests, prepared statements and describe"
  - drops: "Describe names the catalog version it is drawn from"
  - drops: "A client's Describe naming a catalog version is refused"
  - drops: "A Describe continues across frames"
  - drops: "A Describe past the catalog's bound is refused"
  - adds: "A Describe past the client's frame is refused"
  - adds: "Rows come partition by partition"
- `wire-protocol`, "Handshake, streams, credit and cancellation"
  - adds: "A Welcome announces the longest value"
- `wire-protocol`, "Roles exchange role commands"
  - drops: "A list larger than a frame continues across frames"
  - drops: "A Scan's `after` is read from the frame"
  - adds: "A row larger than a frame continues across frames"
  - adds: "A Scan's range bytes are read from the frame"
  - adds: "A range's fields are checked against its space"
  - adds: "A resume key rides only on a range that stopped"
  - adds: "Widths stay within a range"
- `write-ahead-log`, "A round's record is logical"
  - drops: "Replay derives the same keys"
  - adds: "Replay derives the same state"
- `write-ahead-log`, "Checkpoints trim the log"
  - adds: "Replicas checkpoint at the same positions"
  - adds: "A crash before the trim leaves the checkpoint in force"
  - adds: "A clean shutdown takes no checkpoint"
- `write-ahead-log`, "Recovery replays the log above the store"
  - adds: "A build interrupted by a crash resumes from the checkpoint's record"
