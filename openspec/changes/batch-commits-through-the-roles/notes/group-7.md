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
