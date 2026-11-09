# Group 7: performance keeping the guarantees

## 7.1 The baseline split

2026-11-09, on group 6's storage machine (Debian, 6 CPUs, 8 GiB, its data on an ext4 volume; `perf` 6.12), from `152f1aa0`.
`perf record -e cpu-clock:u -F 999 -g --user-callchains` as the roles' user, user space only (`perf_event_paranoid` is 2). In
process: `wal_bench_profile` under `perf` for its whole 15 s run at 1 and 6 partitions (the machine has 6 CPUs, so 6 stands for
the task's 12), `--rate 50000 --cross-percent 0`. Through the binaries: group 6's 6/6/2 layout, one partition, 16 connections
(X1's point), `ultragraphdb_storage_profile` sampled for 10 s of a 25 s run. 1, 10 and 50 relations a commit; one run each.

Each sample of the commit thread (`wal_bench`'s partition threads, storage's `partition-1`) goes to the innermost phase on its
stack: validation (`resolver`, `conflict_history`), the log (`ultragraphdb_transactions_log`), apply (`ChangesetCore`'s round
write and apply, `applier_operator`), execution (`ultragraphdb_transactions_txn`, `store_apply`'s `execution` and
`changeset_write`), the commit step's decision (`commit_step`, `pipeline`, `run`, `sessions`, `epoch_stamp`), the partition's
own code (`ultragraphdb_cluster_partition`, `ultragraphdb_common_storage`), and the rest. Allocation (an allocator at the leaf)
and referrer reads (`referrer_candidates`, `QuadStore::referrers` on the stack) are shares across the phases.

| Where | Relations | Relations/s | Commit thread busy | CPU µs a relation | Execution | Apply | Decision | Validation | Log | Partition | Rest | Allocation | Referrer reads |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| In process, 1 partition | 1 | 16,519 | 85% | 51 | 48.9% | 39.4% | 5.9% | 0.5% | 0.4% | 2.0% | 2.9% | 7.2% | 20.3% |
| | 10 | 22,634 | 86% | 38 | 57.0% | 36.6% | 2.8% | 0.2% | 0.4% | 0.5% | 2.4% | 6.5% | 23.6% |
| | 50 | 26,127 | 89% | 34 | 63.9% | 30.8% | 2.1% | 0.0% | 0.3% | 0.4% | 2.4% | 6.3% | 25.6% |
| In process, 6 partitions | 1 | 77,198 | 4.0 of 6 cores | 52 | 47.6% | 35.1% | 6.1% | 0.5% | 0.4% | 3.7% | 6.5% | 9.5% | 12.0% |
| | 10 | 107,648 | 4.1 of 6 | 38 | 55.1% | 34.8% | 3.1% | 0.1% | 0.3% | 1.9% | 4.7% | 7.9% | 12.9% |
| | 50 | 116,677 | 4.2 of 6 | 36 | 61.5% | 27.7% | 3.8% | 0.0% | 0.3% | 1.9% | 4.9% | 7.8% | 13.1% |
| Binaries, 1 partition, 16 connections | 1 | 6,162 | 47% | 76 | 40.2% | 40.6% | 4.8% | 0.4% | 0.6% | 7.9% | 5.5% | 6.5% | 24.1% |
| | 10 | 12,530 | 77% | 61 | 48.3% | 43.2% | 2.8% | 0.1% | 0.2% | 4.3% | 1.0% | 4.7% | 36.5% |
| | 50 | 14,150 | 81% | 57 | 52.1% | 43.1% | 2.0% | 0.0% | 0.2% | 2.1% | 0.4% | 4.1% | 43.4% |

(CPU µs a relation is the commit thread's samples over the relations committed while they were taken. fjall's workers, flushing
and compacting, take a further 6 to 20% of the process's samples.)

- **Validation, what strict serializability costs, is 0.5% of the commit thread or less** at every point, and 0.0% at 50
  relations a commit.
- **The log's write and sync cost the commit thread no CPU** (0.2 to 0.6%): they are waits. Through the binaries they are wall
  time: at one relation a commit, 16 connections, the partition spends 10% of the run writing its log and 21% syncing it, 5.0
  commits a sync, a sync 180 µs at the mean; at ten relations 4% and 8%, at fifty 2% and 3%.
- **Execution and apply are the commit thread**: 84 to 96% between them. Within them the store: a term's lookup
  (`Dictionary::held_id`) about 9%, the write batch into fjall's memtable about 20 to 22%, lz4 compressing blocks 3 to 5%.
- **Referrer reads are 12 to 26% in process and 24 to 43% through the binaries**, read at execution and again at apply.
- **Allocation at the leaf is 4 to 10%.** It counts the allocator's own time, not the copies and drops around it.
- **A commit costs about 15 µs of the commit thread beside its relations' 35 µs each** in process (fitting 51, 38 and 34 µs at 1,
  10 and 50 relations); through the binaries about 17 µs beside 59 µs each.

### The binaries' 61 µs a relation against the in-process 38

At ten relations a commit the commit thread spends 61 µs a relation through the binaries and 38 in process. The two benches write
differently: `wal_bench` draws its edges' objects from 64 hubs a partition, `client_bench` from 1,000 (`--hubs`), all on the one
partition. The same binaries with `client_bench --hubs 64` (one run, 25 s):

| Hubs | Relations/s | Commit thread busy | CPU µs a relation | Execution | Apply | Referrer reads | of which at apply |
|---|---|---|---|---|---|---|---|
| 1,000 | 12,530 | 77% | 61 | 48.3% | 43.2% | 36.5% | 14.3% |
| 64 | 16,225 | 71% | 44 | 58.7% | 31.4% | 29.0% | 1.4% |

The hubs explain 17 of the 23 µs, most of it the referrer reads at apply, 14.3% of the commit thread with 1,000 hubs and 1.4%
with 64; that 64 hubs' lists stay in the list cache and 1,000 hubs' do not is the likely reason, not counted here. Of the 6 µs left, the binaries' own handling of a frame is about 2 µs: the wire's views 3.5% (1.7% in
process), SipHash over the term table's keys 2.1% (0.6%), the core's one-frame view and replies 1.8%. The rest is the workloads'
difference in referrer reads at execution, 27.6% against 23.6%.

X1's 63 µs a relation through the binaries against an in-process 20 to 25 µs were measured on stage 5.5's VM; on this machine
the in-process commit thread itself costs 34 to 52 µs a relation.

### What each step can gain, from this split

The bound is the commit thread's rate with the step's share gone, 1 / (1 − share). It holds where the commit thread limits the
rate: in process, busy 85 to 91%; through the binaries at 16 connections it is busy 47 to 81%, and the log's waits and the
requests' arrival take the rest.

| Step | Its share of the commit thread | Throughput if it cost nothing |
|---|---|---|
| 7.2 Referrer reads off the write path | 12 to 26% in process, 24 to 43% through the binaries | +14 to +35% in process, +32 to +77% through the binaries |
| 7.3 Allocation per round | 4 to 10% at the allocator, more in the copies around it | +4 to +11% |
| 7.4 Index entries within the KV layout | the write batch into the memtable, 20 to 22%, a part of which the merged entries remove | a part of +25 to +28% |
| 7.5 The owner's epoch-close sync | not on the storage thread; the frontend's checks, 115 µs of a 2.2 ms commit at one client (group 6) | latency only |
| Validation | 0.0 to 0.5% | none to gain |

## 7.2 Referrer sets held in memory: kept

The owner chose to cache (2026-11-09). Each partition's `QuadStore` holds the referrer sets its rounds left, as the store holds them at
the applied watermark, in a `ReferrerCache` of 16 MiB (`REFERRER_CACHE_BYTES`) in two generations of half each: once the young one is
full it becomes the old one, and a set found in the old one moves back. Only a round's apply puts a set, every set it took or
changed, once its batch is written (`ChangesetCore::write_round`), so a held set is the stored one; a drop of a tenant or its data
forgets the tenant. `referrer_candidates` takes a held set before reading the store at its snapshot: a held set is at least as new,
and sets only grow, so a group it names is named at the commit. `derive_referrers` starts a set from the held one, else from the
store at the watermark. A set larger than half the budget is not held.

Tests, each shown to fail with its part taken out: the cache's own (a held set answers, a replacement, the two generations, a set
moved back from the old one, a set too large, a tenant forgotten in both generations); a round holds the sets it writes as the
store holds them; an execution takes a held set naming its group as named (`Executed::referrers`, new, reads the candidates); an
apply adds to the held set in place of the stored one; both drops forget the tenant's sets, and an edge after a data drop writes its
object's set again. `ultragraphdb_store_apply`'s workload differential, faults, replay and seeds suites pass.

Measured as 7.1's machine, the tree before (`152f1aa0`, g6) and after (72) interleaved, three runs a point, medians with their
ranges. In process, `wal_bench_profile`, `--rate 50000 --cross-percent 0`:

| Partitions | Relations | Seconds | Before, relations/s | After | Change |
|---|---|---|---|---|---|
| 1 | 1 | 3 | 19,688 (18,776–20,236) | 21,700 (21,686–22,092) | +10.2% |
| 1 | 1 | 30 | 16,622 (16,194–16,646) | 22,284 (20,426–22,716) | +34.1% |
| 1 | 10 | 30 | 24,362 (24,247–24,882) | 30,991 (30,790–31,008) | +27.2% |
| 6 | 1 | 3 | 100,099 (97,685–106,434) | 108,705 (107,918–108,886) | +8.6% |
| 6 | 1 | 30 | 81,924 (72,773–83,848) | 90,403 (84,102–90,858) | +10.3% |
| 6 | 10 | 30 | 114,894 (103,029–115,806) | 127,006 (124,977–127,192) | +10.5% |

The decay is gone: one partition at one relation a commit fell from 19,688 relations a second over 3 s to 16,622 over 30 s, and now
holds 21,700 and 22,284. Through the binaries, 6/6/2, six partitions, a fresh cluster a run:

| Relations | Connections | Before, relations/s | After | Change | p50 ms | Commits a sync |
|---|---|---|---|---|---|---|
| 1 | 1,024 | 38,948 (38,947–39,632) | 41,452 (40,625–43,650) | +6.4% | 19.0 → 19.2 | 33.8 → 36.2 |
| 10 | 256 | 88,000 (87,330–89,010) | 105,130 (104,970–105,150) | +19.5% | 19.0 → 16.6 | 9.1 → 10.2 |

Every figure lies above the before range. Kept.

`make test` after 7.2 failed once in `ultragraphdb_tests`: "A hung compute process answers code 17" measured 149 ms against its
150. A read's deadline counts from the frontend's Begin (`client_session_requests.rs`), and the step started its clock once the
Begin had returned, missing its way back, so a prompt timer read as early. The clock now starts before the Begin is sent. `make
test` then: doc 21, unit 79, BDD 37, integration 7, all pass.

## 7.3 Allocation per round: measured, not built (the owner, 2026-11-09)

`alloc_bench --frames 10000` on this Mac (`-c opt`) at `7cc1d1ab`, medians a frame:

| Relations | Terms | Allocations: execution | at commit | in all | a relation | a term | Reallocations | Bytes |
|---|---|---|---|---|---|---|---|---|
| 1 | 5 | 94 | 144 | 244 | 244 | 48.8 | 36 | 47,560 |
| 10 | 23 | 672 | 1,002 | 1,682 | 168 | 73.1 | 134 | 313,801 |
| 50 | 102 | 3,283 | 4,831 | 8,125 | 162 | 79.8 | 406 | 1,509,769 |

The allocator and `memcpy` at the leaf are 5.0% of the commit thread's samples through the binaries (7.1, ten relations a commit)
and 8.5% in process at six partitions. No caller holds more than 0.6%. Through the binaries the largest of this repository's are
`MutableLayer::insert_posting` 0.31%, `Request::get_from_delta` 0.26%, `ChangesetWrite::finish` 0.26%, `Request::into_writes`
0.23%, `ChangesetCore::settle_round` 0.22%, `ReferrerSet::set` 0.21% and `PostingList::encode` 0.20%; fjall's own (skip-list nodes,
`ByteView`, its iterators) about 1%. With every allocation of this repository's gone the commit thread would gain at most about 5%
through the binaries, and buffers the pipeline lends would take part of that, near the runs' spread. The owner closed 7.3 without
building it.

## 7.4 Index entries per relation: counted, nothing merged (the owner, 2026-11-09)

A probe (not kept) in `ultragraphdb_store_apply`'s harness: 64 hubs, then 200 commits of 10 relations of `wal_bench`'s shape (a
new node with an edge `rel_t<j>_org` to a hub and the string `rel_t<j>_w` "x", ten relation types), the store's versions counted
by keyspace before and after:

| Keyspace | Versions a relation | New keys a relation | What writes them |
|---|---|---|---|
| Forward | 2.00 | 2.00 | the node's two lists, one per quad |
| Reverse | 2.00 | 0.17 | the hub's reverse list under its predicate, and the literal's |
| SubjectPredicates | 1.00 | 1.00 | the new node's predicate list |
| ObjectPredicates | 1.10 | 0.03 | the hub's and the literal's predicate lists |
| Statistics | 2.00 | 0.01 | a delta per predicate a commit touches |
| Referrers | 0.98 | 0.03 | the hubs' sets, still gaining groups in a run this short |
| In all | 9.08 | 3.24 | |

Of the 9.1, 5.0 are what the reads need: the forward and reverse lists and the new node's predicate list. Two writes repeat what
the store holds:

- **The object-predicate list.** An insert adds its predicate's SET to the object's list blind (`QuadWrite::insert`), so the hub's
  and the literal's lists take a version each commit though they named every predicate long before. Leaving the SET out where the
  list names the predicate needs the list read, and a read that is not recorded would let a concurrent delete of the object's last
  subject under that predicate take the predicate out after this write added a subject: the read is recorded, and the delete then
  conflicts with the write.
- **Statistics.** Each commit writes one delta per predicate it touches. A Describe reads them merged up to its version
  (`QuadStore::statistics`), and a read may name a version inside a round, so one delta a round would change what it returns there.

Neither keeps every read as it is, and the write batch is 20 to 22% of the commit thread (7.1), so the two together are worth about
5 to 9%. The owner chose to record them for roadmap stage 6.5, whose storage format decides how a predicate list and its statistics
are kept, and to merge nothing here.

## 7.5 The owner answers a commit's checks while its logs sync: kept (design D17)

The owner chose to answer during the syncs (2026-11-09). `MetadataGroup::syncing` gives the log's sync beside `MetadataReads`, the
durable state and the head's registry, and `synced` applies what it made durable; the owner keeps the sequencer's last durable close;
while either log syncs, `while_syncing` answers a gate's facts, a commit's owner check and end, a session check and whether a session
is held, and keeps every other ask for the next turn, in order. The sync gate of 3.5 moved, `git mv`, from
`ultragraphdb_common_storage`'s tests to `ultragraphdb_transactions_log`'s `utils_test` behind `test-utils`, so the frontend's tests
hold a log's sync too. Tests: the metadata group's reads during a sync, its registry, and a sync never applied; the owner holding the
sequencer's sync and the metadata group's, each answering the facts and the check while a registration waits; both owner tests fail
with the waits taken out. The client-endpoint requirement "One owner answers for the metadata group" states it.

The 6/6/2 layout, six partitions, the 7.2 storage under both frontends (g72 before, 75 after), three runs a point interleaved:

| Connections | Commits/s | p50 ms | p90 ms | p99 ms | Mean ms | Checks, mean / p99 µs |
|---|---|---|---|---|---|---|
| 1, before | 362 (296–365) | 2.52 | 3.69 | 5.34 | 2.75 (2.73–3.36) | 259 / 1,874 |
| 1, after | 384 (380–390) | 2.44 | 3.20 | 4.60 | 2.59 (2.55–2.62) | 112 / 275 |
| 16, before | 9,191 (8,767–9,638) | 1.48 | 2.25 | 6.64 | 1.74 (1.66–1.82) | 143 / 2,406 |
| 16, after | 8,677 (8,320–9,866) | 1.47 | 2.58 | 7.41 | 1.84 (1.62–1.92) | 68 / 191 |
| 1,024, before | 43,833 (42,949–46,489) | 16.55 | 42.46 | 105.51 | 23.34 (22.00–23.82) | 802 / 6,239 |
| 1,024, after | 42,847 (42,460–43,958) | 14.38 | 52.99 | 115.52 | 23.87 (23.27–24.07) | 202 / 3,119 |

One connection commits 6% more, outside the spread; its checks' p99 falls from 1.9 ms to 0.3 ms. At 16 and 1,024 connections the
rate is storage's and moves within the spread. At 1,024 the median falls 2.2 ms and the p90 rises 10.5 ms, each outside its
range, and the mean does not move: it is the connections over the rate, 1,024 / 42,847 a second, as Little's law has it, and the
clients' own means match it to 0.05 ms. The frontend's phases (partition p50 11.0 against 12.0 ms, total p99 102 to 118 against
100 to 142 ms) and storage's read-to-answer show no added time. Under a fixed mean the change moves where the commits wait, not how
long: before, the owner's syncs held the checks back in batches; now they reach storage's queues, which serve them less evenly, a
reading the phase logs fit and that no run isolated. Kept: the rate rises where the frontend bounds it and nothing loses rate.

## 7.6 The improvements together

Two of the four were kept: the referrer sets held in memory (7.2) and the owner answering a commit's checks during its syncs (7.5);
7.3 and 7.4 were measured and not built. Each one's gain alone is in its section. Together, against group 6's binaries
(`152f1aa0`), on group 6's matrix, the 6/6/2 layout, six partitions, interleaved, three runs a point, at `6753d3ad`:

| Relations | Connections | Group 6, relations/s | Both, relations/s | Change | Share of in-process |
|---|---|---|---|---|---|
| 1 | 256 | 26,857 (26,808–26,949) | 29,710 (29,298–32,118) | +10.6% | 33% → 36% |
| 1 | 1,024 | 40,413 (39,945–40,441) | 41,472 (39,737–43,348) | +2.6%, within the spread | 49% → 51% |
| 10 | 256 | 85,640 (84,430–85,770) | 104,340 (96,960–105,450) | +21.8% | 81% → 98% |
| 10 | 1,024 | 98,590 (79,710–100,330) | 116,210 (107,320–117,480) | +17.9% | 93% → 109% |

The in-process rate is `wal_bench` at HEAD on the storage machine. HEAD's share of it at 1, 4 and 6 partitions, at one relation and
1,024 connections and at ten and 256: 78% and 84%, 44% and 88%, 51% and 98%. Validation is 0.8% of the commit thread or less at
every point; through the binaries at one partition and 16 connections the commit thread spends 46, 32 and 31 µs a relation at 1, 10
and 50 relations a commit (61 at ten in 7.1), the store 16.5 to 20 µs of it, and fjall's workers 25 to 27 µs more at 10 and 50.
Every table, the latency at 1 to 1,024 clients and how each was run are in `openspec/benches/benches-2026-11-09.md`, the baseline
roadmap stage 6.5 is measured against.

## 7.7 What remains for roadmap stage 6.5

The commit thread fits 30.4 µs a relation and 16.0 µs a commit. Of a relation, the KV store takes 16.5 to 17.5 µs on the commit
thread at 10 and 50 relations a commit, 12.5 to 13.3 µs of it writing (the write batch into the memtable, 5 to 6 µs of that lz4
compressing blocks) and 3.7 µs reading (term lookups mostly), and its workers 25 to 27 µs flushing and compacting: 42 to 44 µs of CPU
a relation. A relation writes 9.1 versions (7.4), 5.0 of them the reads need; the object-predicate SETs and the statistics deltas
are the rest but for the referrer sets. Recorded in the backlog as the case for the tile-based storage engine.
