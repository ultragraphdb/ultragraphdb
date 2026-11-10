# UltraGraphDB Specification Consistency Review

Nov 10, 2026 · review of `specification/storage/STORAGE.md`, `specification/schema/SCHEMA.md`,
`specification/binary_protocol/PROTOCOL.md` and `openspec/notes/UltraGraphDB Scalable Tile Store.md` against each
other, after decisions T42–T47. Every decision below was adopted on 2026-11-10 and is the design note's decision T48.

## Method

Five reviewers each read the four documents through one concern: the write path and log, the read path and lanes,
identity and layout, engine internals and settings, and references and terminology. Each concern was checked with the
*Miniature Guide to Critical Thinking*: the elements of thought (purpose, question, information, inference, concepts,
assumptions, implications, point of view) measured against the intellectual standards (clarity, accuracy, precision,
relevance, depth, breadth, logic, significance, completeness, fairness). The reviewers reported about 120 findings,
which collapse to the 48 decisions below and a set of mechanical corrections. Two findings were re-read against the
text before this note was written, the full-grid deadlock (C10) and the session table's key (C3); both hold.

A finding is **mechanical** when the documents already decide the matter and some text says otherwise: a stale
reference, KV-era vocabulary, a design-note paraphrase the specification has superseded. Those are corrected without a
decision (*Mechanical corrections*). Every other finding needs a rule chosen, and each gets a recommendation by
TigerBeetle's principles: the state is a function of the log, every resource has a limit, a check finds its state from
the request alone, one fact lives in one place, and a bad state is made unrepresentable rather than detected.

Severity: **B** an implementation built from the documents is wrong or two implementations diverge; **A** two
readings exist; **W** wording.

## Decisions

### Durability and determinism

**C1 (B). Apply cannot derive what the round does not carry.** STORAGE §15.1 says apply reads the round and the delta
and never a tile, and that execution's derivations are "in the round's record" (T33). The round has no section for
them. A count index's old degree, a statistics record's prior value, a referrer set's members, a `@limit` aggregate and
an existing subject's slot are all needed. Three implementations would read tiles in apply, keep unsized state
resident, or invent a format.
*Recommendation:* execution derives, the round carries, apply adds. Statistics, `@limit` aggregates and referrer-set
additions become additive records that compaction folds, so apply needs no prior value. The Round gains a derivations
section holding each count row's degree before the round. Apply reads slot maps through the cache: execution pins the
pieces a round names until the round applies, and recovery loads them before it applies a replayed round, which may
wait on I/O without changing any decision.

**C2 (B). A purge hides nothing below its version.** STORAGE §15.5 writes a purge as deletes at the purge's version,
which §14.4 shows to reads below it; SCHEMA §8.8 says no read at any version sees purged data, and a class I tile has
no deleted versions to carry them. A Purge record names only IDs, so apply cannot list what to delete.
*Recommendation:* a purge marker, record kind 20, keyed by the instance's system ID with the purge's version: reads at
every version treat every entry naming the instance as absent, rows, neighbours, columns and records alike, and
compaction removes them, class I included. A referrer member receives the marker through the purge's share.

**C3 (B). The session table has three keys and an unlogged eviction.** PROTOCOL §11.5 keys it by session, §6.8 and
STORAGE kind 10 by (session, share), the note's T23 by (session, request, share). Share numbers follow the placement
map, so a resend after a map change reaches a partition under another share number and commits again. Evictions arrive
with epoch closes and change stored records at no logged position, so replicas diverge. Kind 10 cannot hold a Drop
reply or rebuild a reply's command header.
*Recommendation:* key by session alone; the value holds the last request, the share it was, the reply's command, its
128-byte command header and its body. The Round gains a section of the sessions evicted at that round, the first after
the close is delivered, applied as record deletes.

**C4 (B). Builds recover two ways and race the fold horizon.** PROTOCOL's recovery restarts a build from its start
version, which a trimmed log no longer holds; STORAGE §16.7 resumes from its record (kind 15). Nothing keeps `H` below a
running build's start version, so folding can double-index entries.
*Recommendation:* a build resumes from its record as the checkpoint holds it; `H` takes the start version of every
build in progress as a further minimum; the walk reads its source at the start version.

**C5 (A). Replay depends on catalog versions the log does not hold.** Tile bytes follow the Layout entry of the catalog
version in effect, which lives in the metadata group.
*Recommendation:* STORAGE §2 lists the catalog versions the rounds name, immutable once committed, among apply's
inputs; the metadata group keeps every version a partition's log above its last checkpoint names.

**C6 (A). When a checkpoint's frees take effect depends on I/O timing.** The next placement's lowest free extent
depends on whether step 6 has completed.
*Recommendation:* the frees of a checkpoint take effect at the boundary of the first round after its position, and
that boundary waits for step 6 when it has not completed, so placement reads a function of the log.

**C7 (A). A new grid's first checkpoint has two readings.** *Recommendation:* a new grid holds no checkpoint; recovery
replays from position 1, and the first checkpoint is the first section 17.1 makes, as the reference engine does.

**C8 (A). A bucket group's move leaves ages and the source undefined.** A moved tile's `built` is a source-log position,
so the destination's stream rule subtracts positions of two logs.
*Recommendation:* the source completes the group's rewrites at the move boundary, as the drain does; a rewrite whose
first input was moved in takes `previous` 0; the source drops the group's tiles at the move position. Stage 8 builds it.

**C9 (A). Adopting a peer's checkpoint fails the identity check.** *Recommendation:* stage 8's state sync writes the
adopting replica's own superblock: its `replica`, its next `sequence`, its last superblock as `parent`; the identity,
which excludes all three, is the peer's.

### Admission and bounds

**C10 (B). A full grid deadlocks.** Admission counts free and released extents, but a placement needs a free one, and
released extents become free only at a checkpoint, which needs log growth that stopped admission prevents. The
cleaner's floor, 5% of 64 extents, is 3, below admission's 8. One checkpoint's drain rewrites nearly every tile holding
changes, which 8 extents do not bound.
*Recommendation:* admission counts free extents only and reserves the drain's worst case to the next checkpoint (the
blocks of tiles holding changes, plus the manifest and free set). A primary whose partition is held by `GridFull` logs
a round flagged *checkpoint*, which section 17.1 takes as a checkpoint position, so a logged fact frees the released
extents. `cleaner_free_percent`'s floor becomes the reserve's share of the grid.

**C11 (B). One round can overrun the delta pool.** Above the watermark the headroom is as little as 3,277 entries; one
1 MiB round derives more.
*Recommendation:* admission is checked per round as it is built, counting the round's own changes, and the headroom's
lower bound is stated against the round bound.

**C12 (B). The store's refusals have no wire codes, and the value bound is unannounced.** `ValueTooLarge`, `GridFull`
and delta admission map to no code; PROTOCOL promises a storable 262,143-byte vector where STORAGE refuses 16 KiB.
*Recommendation:* `ValueTooLarge` is code 6 (TooLarge), `detail` the bound; `GridFull` and delta admission are code 5
(Busy), retryable, `detail` naming which; the Welcome announces `value_bytes_max`, `tile_bytes_max / 4`; FIXED sections
take widths up to it, so a vector is a fixed-width column; SCHEMA states the bound and its refusal beside `UGS0204`.

**C13 (A). `max_ingest` exists in STORAGE and the note but not in PROTOCOL.** *Recommendation:* drop it; a share's
bound is PROTOCOL §6.8's round fit, code 6.

### Identity, claims and records

**C14 (B). Slots collide across types in one family.** Slots are dense per (bucket group, type), but forward, reverse
and index families carry no type, so slot 3 of two types is one row or one neighbour.
*Recommendation:* a slot row key and a slot neighbour become (type prefix, slot), 6 bytes, the prefix in its own FOR
section as kind 1's neighbours already store theirs; columns stay dense per type (T3). Tile bytes change and the
vectors are regenerated.

**C15 (B). Key claims have no graph; SCHEMA makes keys unique per graph.** STORAGE kind 4 also names "the type prefix"
where SCHEMA hashes the key space's.
*Recommendation:* kind 4's key is the key space's prefix, the graph's system ID, then the canonical key, and conflict
family 7 takes the graph; kind 13 names the concrete type's prefix; kind 12 the root's.

**C16 (B). Class I dictionary and idempotency records contradict deletes.** Re-creating a deleted IRI needs a second
term-to-ID record while the first stays visible; an idempotency claim of a deletable instance outlives its delete; no
record holds "the ID it replaced" that SCHEMA §8.5 promises.
*Recommendation:* kind 1 becomes class M, so a re-creation deletes the old record at its version and a read below it
still resolves the old ID; kind 13 takes its type's class and a delete releases it; kind 2's value appends the replaced
ID, 16 zero bytes for none.

**C17 (B). `@unique` claims sit in no bucket group and move wrongly.** *Recommendation:* kind 5's group is the bucket
group of `Bucket::of_value`, and it moves with that group like every grouped run; in the generic layout its value is
the term encoding.

**C18 (A). Copied dictionary entries take a group the partition does not own.** *Recommendation:* a copy of another
partition's entry lives in a family with group `ABSENT`, the replica's own, never moved; a move sends the group's own
entries.

**C19 (B). The catalog cannot say which attribute is `@idempotency`.** *Recommendation:* CATALOG §5.4 Attribute
`flags` bit 4, with the decoder's rule of one per type, inherited ones counted.

**C20 (A). A keyed relation has no anchor, yet SCHEMA homes its `@set` and `@idempotency` claims there.**
*Recommendation:* SCHEMA says "the instance's partition: an entity's own bucket, an unkeyed relation's anchor, a keyed
relation's key bucket", as STORAGE already does.

**C21 (A). `@set` and `@limit` record keys are underspecified.** *Recommendation:* kind 12, node-encoded only: the root's
prefix, then each role in order, its participants ascending, a zero count for an empty optional role; an edge-encoded
`@set` relation's combination is its triple. Kind 19: the declaring prefix, the constraint's index, each `per`
participant in `per` order, then the concrete prefix when `per` names `type`; a `@limit` check records the aggregate's
key in conflict family 7.

**C22 (A). A statement record is keyed by ID; the log keys it by graph and ID.** *Recommendation:* a statement ID names
one quad, so kind 18 stays keyed by the ID and PROTOCOL §12.2 says so.

### Compiled layout and encodings

**C23 (B). Every edge-encoded relation would become a column.** STORAGE §13.4's "single-valued anchor role" is true of
every edge-encoded relation under SCHEMA's definition, so `knows*` would hold one value per anchor.
*Recommendation:* a column chunk only when the anchor's navigation cardinality is at most one (`?`, `{0,1}`), holding the
other participant's ID.

**C24 (B). A `List` is stored as a sorted set.** *Recommendation:* a single `List<T>` attribute is a column chunk of
kind 2 holding tag 25's whole value; only `Set<T>` takes a forward family of kind 2 per element.

**C25 (B). Temporal values cannot order a row and equal instants claim twice.** Dates and times are 18 bytes, the
inline cap 16, so T16's newest-first order is unbuildable; `12:00Z` and `13:00+01:00` are one value but two `@unique`
claims. T16's rule lives only in the note.
*Recommendation:* keep the 18-byte encoding, so the offset round-trips; order, equality, claims and tokens compare its
first 16 bytes, the UTC instant; inline columns take up to 18 bytes; T16's eligibility, direction and tie-break move
into STORAGE §13.4, with the reverse family's denormalised column.

**C26 (B). `Text` loses its language tag.** *Recommendation:* tag 15's encoding is `esc` of the text, then the
lowercased BCP 47 tag, empty for none.

**C27 (B). STORAGE stores WKT, SCHEMA GeoJSON.** *Recommendation:* GeoJSON, canonicalised by RFC 8785 (JCS), which
every major language implements; PROTOCOL §10.5 names it.

**C28 (B). Range tokens are lossy in SCHEMA, exact in STORAGE.** *Recommendation:* a range token is the identifier byte
and the sortable encoding (T37); SCHEMA drops the `hour` and `day` tokenizers; a `Measure<K>` token is the value in the
quantity's canonical unit as `Float64`.

**C29 (A). An `Iri` value is stored by system ID, though SCHEMA says it creates no entity.** *Recommendation:* tag 23
is the IRI's UTF-8 text, variable width; tag 35 alone stores a system ID.

**C30 (A). Interval bounds, precision and `@bitemporal`'s `asserted` have no families.** *Recommendation:* a precision
tag (CATALOG tag 37, 1 byte); STORAGE §13.4 rows for an interval's four bound attributes and for each `@bitemporal`
type's `asserted_start_epoch` (class I) and `asserted_end_epoch` (class M) columns, with built-in predicates in SCHEMA
§7.4.

**C31 (A). A generic reverse family mixes raw IDs and term encodings as row keys.** *Recommendation:* every generic
reverse row key is a term encoding, an IRI or blank node as its tag and system ID; tile bytes change.

**C32 (A). SCHEMA promises lookup by value for every attribute; the layout keeps none for columns.**
*Recommendation:* SCHEMA says a value is found through its index or claim when declared, otherwise by a Scan of the
attribute's family; no reverse family is added.

**C33 (A). Monotonic-fast reads over `@immutable` values of deletable instances are not lasting.**
*Recommendation:* SCHEMA §9.6 says `@append_only` data and the `@immutable` attributes of `@append_only` instances.

### Reads

**C34 (B). TooOld has two mechanisms.** STORAGE folds at a logged `H` with `retention_ms` 30 s; PROTOCOL §11.9 and the
config service keep a per-node `read_retain_ms` of 60 s from a local clock. A read-only transaction's Scan 45 s in is
promised by one and refused by the other.
*Recommendation:* the logged `H` is the only retention: PROTOCOL drops `read_retain_ms` and the discard version, TooOld
is "below the latest logged fold horizon", a pin holds `H` on the primary only, and a follower may answer TooOld within
a request; the config service drops `storage.read_retain_ms` and adopts STORAGE's bound.

**C35 (B). Frontier's answer order and resume contradict the family order.** *Recommendation:* rows come in the
request's order, each row's entries in its family's order; the resume key adds the row's index and the survivors a
`top` row has answered; a request naming a row twice is refused.

**C36 (B). Reverse and index rows fall in no bucket group.** *Recommendation:* a forward or column row is read in its
subject's group; a reverse or index row in every group of the replica and both classes, groups ascending, each in its
family's order.

**C37 (B). Scan cannot give compute an ordered family's ordering value.** *Recommendation:* an Entries object record of
a family with inline columns carries their values, as a Neighbours frame does.

**C38 (B). An unbound predicate has no read and no phantom protection.** *Recommendation:* Scan space 2 and Frontier
take `predicate` 16 bytes of `0xFF` for every predicate of the kind; in lane X such a read records its graph's range.

**C39 (B). No read reaches a version run.** *Recommendation:* kind 5 joins Frontier and Scan: the row an instance, the
entries (asserted start, value); compute selects the revision as of its time.

**C40 (A). Which lane a Frontier takes per level is unstated.** *Recommendation:* a table in PROTOCOL §13.12: a
read-write transaction X at the Bind's version; level 0 S at the snapshot's bound; level 1 S at its first Scan's
version; level 2 S at the bound it chose; L only for a read classified monotonic-fast.

**C41 (B). The `Lane` refusal has no rule.** SCHEMA never says which lane a family allows; lanes A, V and C exist only
in the note. *Recommendation:* drop the refusal: every family allows L, S and X; A, V and C are named as the commit
step's paths in the transactions specification, not storage lanes.

**C42 (B). A stalled Frontier pins `H` for ever.** *Recommendation:* `scan_stall_ms` bounds every Scan and Frontier.

**C43 (A). What lane X records differs between STORAGE and PROTOCOL.** *Recommendation:* PROTOCOL is the rule and
STORAGE §18.3–18.4 cite it; a found class I entry is never recorded, in any read, and absence always is; an absent
statement record is recorded as an AbsentTerm on its ID.

**C44 (A). No role joins a relation's participants' liveness.** *Recommendation:* compute filters relation rows by each
participant's `asserted_end` through Frontier reads at its home, and in lane X those reads record the participant's
forward row.

### Ingest

**C45 (B). Apply cannot tell which Ingest rows execution skipped.** An edge already visible, or a row deduplicated by
`@idempotency`, is logged in the body unchanged. *Recommendation:* the resolution lists the skipped (block, row) and
(block, entry) indices, and apply skips them; a second row of one share with the same `@idempotency` value names the
first row's instance.

**C46 (B). Ingest's slot order is undefined.** *Recommendation:* STORAGE §14.6 adds blocks in order, rows in order, a
row's subject before its neighbours, then the resolution's statement IDs; a statement ID takes its slot where a record
first names it.

**C47 (A). Relations, DirectWrite and sessionless frames under T42, T43 and T47.** *Recommendation:* an Ingest group
holding a node-encoded relation instance goes to its anchor; only a keyless entity's `@idempotency` value places a
group; DirectWrite refuses an `@idempotency` claim as it refuses a key's; a sessionless Ingest sends its unplaced groups
to one partition the frontend chooses in turn for the frame.

### Terminology

**C48 (W). Words with two meanings.** *Recommendation:* PROTOCOL §12.2's "family" becomes "conflict family"; the
write-set record's `kind` becomes "entry kind"; a pattern's slot becomes a "pattern slot" and the note's "4 KiB slots"
become blocks; the superblock's `watermark` becomes `committed`.

## Mechanical corrections

Applied without a decision, each where the documents already decided the matter:

- **References.** The decision ranges in STORAGE's header and the note's section 6; STORAGE §21's asks, all of them
  met, reduced to one sentence naming the specifications that carry them; bare section numbers that named another
  file's section (STORAGE §5.1 and kind 10, CATALOG's `local`, the note's §4.17); a legend for T*n* tags in PROTOCOL §1
  and CATALOG §1; E5 measuring the aborts T44 cites it for, E10 the drain's boundary; PROTOCOL's error form in STORAGE
  §18.3.
- **KV-era vocabulary.** PROTOCOL's tenant isolation, `statements_per_object_max`, code 17, §6.5's drops, §12.2's
  conflict statements, §13.12's corruption rule, §13.13's term rationale and the build swap; the note's reverse and
  forward lists and its subject-predicate and object-predicate records.
- **Wording.** A round never spans a close; an Ingest's token in §4.8's refusal; the tombstone's record kind; result
  rows against store rows; §9.1's reasoning limited to a Frontier; §13.12's error codes; the lane bounds bounding every
  Frontier frame; STORAGE §2's repair exception, §19.5's identical grids, §20's replay bound and its class-split row.
- **The note against STORAGE.** Checkpoints, the manifest, the superblock's one position and selection rule, the tile
  header's 128 bytes, one checksum per tile, no compression and no blob extents in format 1, two death positions,
  512-byte torn writes, references, cuts and pacing, the fold horizon's formula, the delta pool's capacity, resume
  keys, drops, T37's widths and the storage-class row.
- **Other specifications.** The config service's `grid_extents` bound; the write-ahead log's power-loss flag setting
  round bounds only; the storage conformance README's whole-grid hashes; the reference engine running the cleaner once
  at a checkpoint boundary, after the drain.
