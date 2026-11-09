# UltraGraphDB Shard-per-Core Write Path

Preparation for stage 5.5's specifications: what bounds a replica group's write throughput, measured on 2026-11-01,
and the runtime model the measurements point to. A storage node gives its cores to partitions, each a shared-nothing
replica group with its own log, commit step and buckets; every partition core hosts three replicas, leading one, after
ScyllaDB's shard-per-core model and TigerBeetle's one core per replica. The model is sized for nodes of 12 to 128
cores. Stage 5.5 built what P20 and P24 give it, listed with its crates and files in *What stage 5.5 built* at the
end; backlog item 11 tracks what stage 8 builds. The distribution architecture
(`UltraGraphDB Distribution Architecture.md`) is the frame it refines, and the distribution protocol
(`UltraGraphDB Distribution Protocol.md`) is the replication protocol these replicas run.

## Measurements

All figures come from `wal_bench` (the stage-5 pipeline: execute, validate, log, apply) on the OrbStack VM
`ugdb-linux`: aarch64, 12 vCPUs on a host with 12 performance and 4 efficiency cores, glibc with mimalloc, data on the
VM disk (`/var/tmp`). The VM's `/tmp` is a RAM-backed tmpfs where `fdatasync` costs about 1 µs, so runs there
overstate throughput. Each commit of `r` relations creates `r` new nodes, each with an edge to one of the hubs and a
string property.

### One commit thread

| What | Figure |
| --- | --- |
| Relations per second, one pipeline | about 38,000 at 10 and 50 relations per commit; 16,700 commits/s at 1 |
| CPU on the commit thread | about 35–60 µs per commit plus 20–25 µs per relation |
| Store operations per relation (50 per commit) | 2.0 dictionary point reads, 1.0 of them a miss (the new node's IRI); 5.05 entries written; no snapshot reads |
| Cost per operation | about 3 µs per point read, about 1 µs per memtable insert |

The profile (`wal_bench_profile`, `perf record -g`, 50 relations per commit) splits the commit thread's time:

| Step | Share | Runs in parallel? |
| --- | --- | --- |
| Executing transactions (`Transaction::write`) | 39%, the dictionary's IRI lookups 24% | Yes: each transaction reads its own snapshot |
| Preparing commits (`Transaction::commit`) | 8% | Yes |
| Applying rounds (posting writes, fjall's batch commit) | 37% | Yes, by key range or keyspace: the memtable is a concurrent skiplist |
| Validation and `Execution::finish` | 11% | No: the commit step's serial core |

An operation costs microseconds because each is a walk down a large crossbeam skiplist: every level is a cache miss,
and a key longer than 20 bytes adds a second miss to reach its bytes. Dictionary keys are hashes, so successive lookups
share no cache lines. The cost is the number of operations per relation, each paying that walk; the bytes of the keys
add little.

### The stored key and the dictionary cache

Two changes landed in `ultragraphdb_store_kv` on 2026-11-01:

- **The stored key encodes zero runs.** A run of zero bytes takes two bytes however long, in place of two bytes per
  zero byte (`kv-store` spec, *Versions are part of the key*). Dgraph's keys are mostly zero bytes, so the change
  shortens them and cuts the fjall journal by 23% per relation.
- **The keyspace without versions caches what its point reads find**, bounded at 64 MiB, invalidated by every commit
  that writes it and by a drop of everything (`kv-store` spec, *A keyspace without versions*). Lookups of known IRIs,
  hubs and predicates among them, become hash lookups; a new IRI still reads the store.

Measured against the preceding build, three runs each:

| Relations per commit | Before | After | Change |
| --- | --- | --- | --- |
| 1 | 13,357 | 15,696 | +17.5% |
| 10 | 34,161 | 41,217 | +20.7% |
| 50 | 36,397 | 40,491 | +11.2% |

Smaller memtables were tried and rejected: 16 MiB gained 4% at 50 relations and lost at 10, 8 MiB lost 31% to flush
pressure.

### Partitions side by side

N independent pipelines in one VM, each with its own log, fjall instance and commit thread, measure what one
partition per core yields. Relations per second:

| Pipelines | 1 relation per commit | 10 relations | 50 relations |
| --- | --- | --- | --- |
| 1 | 16,706 | 39,043 | 38,501 |
| 2 | 26,400 | 76,677 | 77,047 |
| 4 | 55,755 | 139,509 | 141,021 |
| 6 | 62,806 | 192,503 | 201,654 |
| 8 | 88,189 | 241,292 | 199,514 |
| 10 | 99,412 | 271,578 | 252,924 |
| 12 | 107,130 (6.4×) | 276,836 (7.1×) | 263,196 (6.8×) |

Each pipeline loses 41–47% of its own throughput at 12, at every commit size. The pipelines share no lock, so the
loss is the machine's: 12 commit threads, fjall's flush and compaction workers and the load generators on 12 vCPUs,
and every fsync on one virtual disk. At one relation per commit the fsync per commit dominates and scaling is weaker
from the start. The relations set one core's ceiling; partitioning sets how many cores it multiplies by.

A single commit step fed by execution on every core is bounded by what stays serial. By Amdahl's law from the profile:
about 1.9× with execution off the commit thread, about 2.7× with the apply on a thread of its own, and above that only
with the apply itself split, until validation's 11% bounds it.

### Stage 5.5 on the VM disk (2026-11-08)

The runs of task 12.5, on the machine above: `ugdb-linux`, aarch64, 12 vCPUs, kernel 7.2.12-orbstack, data on the VM
disk (`/var/tmp`, btrfs on the virtual disk `vdb`); `wal_bench` built by Bazel with `-c opt` for
`aarch64-unknown-linux-gnu`, glibc with mimalloc, from commit `b010b64f` (the throughput runs) and from the same commit
with the inbound probe added (the probes); 2026-11-08, 20:44 to 21:56 UTC. Each figure is the median of three
30-second runs at an offered 50,000 commits a second per pipeline or partition, the range of the three beside it in
parentheses; the repetitions were interleaved. The host's 1-minute load was 4 to 5 with the VM idle, from the host's
own background processes, and 4 to 13 during the series, the VM's busy vCPUs counted in it; the VM's own load was 0.2
before the first run and up to 15 after the 12-partition runs. The pipeline's first repetition, started with the VM
idle, and its later ones, started after 12-partition runs, differ by under 4%.

#### One partition against stage 5

| Relations per commit | Stage 5, relations/s (2026-11-01, *One commit thread* and *The stored key*) | Pipeline run, relations/s | `--partitions 1`, relations/s | Log bytes per relation, stage 5 → now | Journal bytes per relation, stage 5 → now |
|---|---|---|---|---|---|
| 1 | 16,706; 15,696 after the stored key | 11,881 (11,561–12,070) | 14,170 (13,937–14,482) | 2,002–2,010 → 1,175 (1,169–1,183) | 1,780 → 1,546 (1,500–1,575) |
| 10 | 39,043; 41,217 | 14,202 (14,019–14,380) | 21,374 (21,316–21,945) | not measured → 736 | not measured → 1,218 (1,204–1,222) |
| 50 | 38,501; 40,491 | 14,235 (14,222–14,250) | 27,640 (22,972–28,063) | 1,100–1,106 → 696 | 1,035–1,036 → 1,159 |

The pipeline run is stage 5's workload through stage 5's path, now over 128-bit IDs: 1,000 hubs, ten relation types,
the predicates declared. Stage 5's bytes per relation are the macOS runs of stage 5's task 12.5 (64-bit IDs, before the
stored key's zero runs, which cut the journal by 23%); a byte count does not depend on the machine's load.

- **The log writes 41% fewer bytes per relation at one relation per commit and 37% fewer at 50.** Its record is the
  logical record of P9 and P18, resolved quads with their system IDs, in place of stage 5's write sets.
- **The journal writes 13% fewer bytes at one relation per commit and 12% more at 50** than stage 5 before the stored
  key, the 128-bit IDs and P24's entries (marks, referrer sets, the local reverse index) against the shorter stored
  key.
- **The pipeline commits 29% fewer relations a second at one relation per commit and 64% fewer at 10 and 50**, outside
  every spread. The profile names the cause: `wal_bench_profile` under `perf record -e cpu-clock -g` (as root, the VM
  having no hardware counters), one 30-second run at 10 relations per commit, 14,236 relations a second. 78% of the
  samples are the commit thread's, 21% fjall's workers. `Store::get_at`, a point read through a snapshot, takes 47% of
  all samples, about 60% of the commit thread: `referrer_candidates` (`ultragraphdb_store_apply`) reads the referrer
  set of every entity an edge points at that the partition owns, here a hub, at execution and again when the round
  applies (32% inclusive), and each read loads and checksums table blocks from the file (`__arch_copy_to_user` 18%,
  `xxh3` 15%, the block index's iteration 14–17%). A hub's referrer set grows to the 2^10 bucket groups and takes a new
  version at each group added, so the reads cost more as a run goes on: a 3-second trial run commits 35,984 relations a
  second at 10 per commit, a 30-second run 14,202. Without those reads the commit thread would spend about 40% of its
  present time per commit, an estimate from the profile that puts the rate near stage 5's. Serving the read from a cache, or skipping it for a group the round
  already names, is a design choice left open here.
- **`--partitions 1` commits more than the pipeline run at 10 and 50 relations per commit** on its own workload: 64
  hubs per partition, the predicates undeclared, each node's string its only other entry. It was not profiled.

#### 1 to 12 partitions in one process

`wal_bench --partitions N`, each partition on a pinned compio runtime with its own store, log and generator, the main
thread closing an epoch every 10 ms. Relations per second, no edge crossing partitions, beside the side-by-side table
above (separate processes, each stage 5's pipeline):

| Partitions | 1 relation per commit | 10 relations | 50 relations | Separate processes, 2026-11-01: 1 / 10 / 50 |
|---|---|---|---|---|
| 1 | 14,170 (13,937–14,482) | 21,374 (21,316–21,945) | 27,640 (22,972–28,063) | 16,706 / 39,043 / 38,501 |
| 2 | 31,996 (30,670–32,433) | 48,340 (47,577–50,574) | 61,691 (60,121–64,581) | 26,400 / 76,677 / 77,047 |
| 4 | 58,017 (41,949–62,444) | 100,579 (94,064–101,442) | 116,770 (108,030–119,750) | 55,755 / 139,509 / 141,021 |
| 6 | 81,352 (80,960–83,424) | 138,485 (129,501–141,749) | 164,296 (153,750–165,837) | 62,806 / 192,503 / 201,654 |
| 8 | 105,504 (88,788–108,776) | 166,750 (154,941–175,392) | 209,891 (183,983–211,709) | 88,189 / 241,292 / 199,514 |
| 10 | 108,974 (104,653–110,597) | 186,567 (171,063–200,340) | 220,642 (205,209–230,396) | 99,412 / 271,578 / 252,924 |
| 12 | 125,391 (124,057–129,360) (8.8×) | 185,630 (179,162–203,082) (8.7×) | 222,105 (216,121–229,102) (8.0×) | 107,130 (6.4×) / 276,836 (7.1×) / 263,196 (6.8×) |

- **One process scales further than separate processes:** at 12 partitions each keeps 67–74% of one partition's rate,
  where each separate pipeline kept 53–59%. The cause was not profiled.
- **The totals at 10 and 50 relations stay below the separate processes'**, since one partition commits less (the
  table above); at one relation per commit they are above from 2 partitions on.
- The rows are the side-by-side table's partition counts; 3, 5, 7, 9 and 11 partitions were not run.
- The log's bytes per relation are each partition's alone and equal at every count: 1,168 at one relation per commit,
  861 at 10, 712 at 50; with the store's files, at 1 and at 12 partitions, 3,806 and 3,960, 2,816 and 2,721, 2,153
  and 2,947.
- With no edge crossing, a partition still registers the predicates and `rdf:type` homed elsewhere, once each: 287 to
  1,843 registrations a second at 12 partitions, averaged over the run.

With 10% of edges pointing at hubs other partitions home (`--cross-percent 10`):

| Partitions | Relations per commit | Relations/s at 0% | Relations/s at 10% | Registrations/s | First-reference wait, p50 (range) / p99, ms | Refused |
|---|---|---|---|---|---|---|
| 2 | 1 | 31,996 (30,670–32,433) | 27,608 (27,086–27,696) | 1,588 (1,574–1,592) | 15 (15–16) / 67 | 0.01% |
| 2 | 10 | 48,340 (47,577–50,574) | 37,827 (37,435–38,064) | 1,906 (1,900–1,906) | 76 (73–76) / 251 | 0.39% |
| 2 | 50 | 61,691 (60,121–64,581) | 42,312 (32,024–50,187) | 1,988 (1,787–2,092) | 257 (253–405) / 727 | 5.11% |
| 4 | 1 | 58,017 (41,949–62,444) | 53,521 (37,706–55,583) | 3,713 (2,938–3,818) | 16 (16–25) / 94 | 0.01% |
| 4 | 10 | 100,579 (94,064–101,442) | 77,892 (76,290–78,511) | 4,874 (4,821–4,892) | 82 (80–83) / 258 | 0.30% |
| 4 | 50 | 116,770 (108,030–119,750) | 72,742 (71,866–80,501) | 4,778 (4,728–5,024) | 374 (272–382) / 764 | 4.66% |
| 6 | 1 | 81,352 (80,960–83,424) | 66,769 (63,688–73,841) | 5,135 (4,944–5,484) | 21 (18–21) / 134 | 0.01% |
| 6 | 10 | 138,485 (129,501–141,749) | 110,577 (106,598–113,759) | 7,533 (7,384–7,640) | 86 (86–89) / 251 | 0.29% |
| 6 | 50 | 164,296 (153,750–165,837) | 103,563 (103,320–106,304) | 7,416 (7,392–7,537) | 397 (383–403) / 615 | 4.43% |
| 8 | 1 | 105,504 (88,788–108,776) | 84,808 (73,554–88,494) | 6,682 (6,005–6,915) | 21 (20–23) / 130 | 0.01% |
| 8 | 10 | 166,750 (154,941–175,392) | 136,977 (123,566–140,935) | 9,857 (9,288–10,024) | 90 (88–100) / 329 | 0.29% |
| 8 | 50 | 209,891 (183,983–211,709) | 133,055 (125,152–158,046) | 9,900 (9,530–10,877) | 398 (300–437) / 774 | 4.49% |
| 10 | 1 | 108,974 (104,653–110,597) | 96,272 (91,380–99,554) | 7,829 (7,518–8,020) | 23 (21–23) / 182 | 0.01% |
| 10 | 10 | 186,567 (171,063–200,340) | 150,115 (142,342–156,794) | 11,500 (11,181–11,860) | 96 (89–104) / 429 | 0.32% |
| 10 | 50 | 220,642 (205,209–230,396) | 150,911 (138,496–157,107) | 11,897 (11,285–12,130) | 434 (417–466) / 904 | 4.54% |
| 12 | 1 | 125,391 (124,057–129,360) | 108,387 (92,092–110,210) | 8,929 (7,906–9,098) | 24 (24–26) / 175 | 0.01% |
| 12 | 10 | 185,630 (179,162–203,082) | 152,569 (141,683–160,877) | 12,565 (11,950–13,000) | 107 (102–115) / 518 | 0.33% |
| 12 | 50 | 222,105 (216,121–229,102) | 158,244 (152,803–165,389) | 13,188 (12,798–13,589) | 477 (445–501) / 1,058 | 4.69% |

- **A tenth of edges crossing costs 8–20% of the relations at one relation per commit, 18–23% at 10 and 29–38% at
  50.** A commit waits for every first reference it makes; at 50 relations a commit holds about five crossing edges,
  and its wait is the slowest home's.
- **The first-reference wait is 15–24 ms at the median with one relation per commit**, one to two epochs of 10 ms:
  the home's durable round, then the close the reply names. It grows with the commit's size, 76–107 ms at 10 and
  257–477 ms at 50, as each home's rounds grow and the round holding the registration syncs later.
- **Registrations run at 6–8% of the relations a second**, most crossing edges: over 30 seconds the 768 hubs of 12
  partitions and their 2^10 bucket groups each, 786,432 pairs, are far from all marked, so most crossing edges are
  still a first reference. P24's bound of one registration per entity and bucket group caps the count over a run's
  life, not within these runs.
- **Refusals appear only with crossing edges:** the commit step's `Conflict` of kind `List`, 0.01% of commits at one
  relation per commit, 0.3% at 10 and 4.4–5.1% at 50. A commit under a registration reply reads its mark (the
  `partitions` spec), so of two commits in flight referencing one entity from one bucket group, the second to commit
  conflicts; the key the conflict names was not recorded. The bench counts these as refused and does not retry them.

#### The inbound point read beside P24's estimate

P24 estimates an inbound point read at "about 20× the store probes of one reverse list at in-degree 20".
`wal_bench --inbound 1000 --partitions 12` writes, for in-degrees 1, 20 and 1,000, an object with that many subjects
pointing at it through one predicate, each subject homed by its bucket and its edge one commit of its partition through
the partitions' hosts and logs, registering at the object's home; then it reads each object's subjects at the last
closed epoch, through `Fanout`, a thousand times, beside the same read on one partition holding every commit. The
probes are counted at the stores: posting lists read (each a cache lookup, a store read on a miss) and point reads.
Three runs with the posting cache at 128 MiB and three without one; the counts were the same in every run, the times
are medians of the three runs' medians.

| In-degree | Partitions asked of 12 | Probes, fan-out | Probes, one partition | Ratio | Time, 128 MiB cache: fan-out / one partition | Time, no cache: fan-out / one partition |
|---|---|---|---|---|---|---|
| 1 | 1 | 3: the referrer set, then 2 lists | 2 lists | 1.5× | 1.2 µs / 0.5 µs | 3.6 µs / 3.1 µs |
| 20 | 9 | 19: the referrer set, then 2 lists a partition | 2 | 9.5× | 6.2 µs / 2.3 µs | 37.4 µs / 17.3 µs |
| 1,000 | 12 | 25 | 2 | 12.5× | 80 µs / 48 µs | 783 µs / 717 µs |

- **Each partition asked reads two lists**, the object's predicates list, which proves the predicate before the reverse
  list is read, and the reverse list; one partition reads the same two. The ratio is `(1 + 2k) / 2` for `k`
  partitions asked, which reaches P24's 20× at `k` = 20: with 20 or more partitions, an in-degree of 20 spreads over up
  to 20 of them.
- **On 12 partitions the 20 subjects fall in 9**, the bench's fixed names; 20 subjects in random buckets fall in about
  10.4 of 12 on average.
- **The time grows less than the probes**: 2.7× at in-degree 20 with the cache and 2.2× without, 1.7× and 1.1× at
  1,000, where decoding the subjects outweighs the lookups. These reads run in one process; across nodes in stage 8,
  each node asked adds a network round, and the referrer set's read at the home one round before them.
- **The writes of the probe** committed every one of the 1,021 edges after 124 to 316 retries of refused commits, the
  same `Conflict` of kind `List`: up to 83 first references to one object from one partition were in flight together.

#### Through the three binaries with the real client

Task 12.5b's runs, on the same VM and disk, 2026-11-08, 22:18 to 23:44 UTC: the role binaries built `-c opt` from
`b010b64f`, started by the supervisor over the `dev` profile (epochs 2 to 10 ms, unpinned, roles over Unix sockets, the
client endpoint over TCP on the loopback), and `client_bench`, the compio client, each connection sending its requests one
after another and timing each to its answer. Each commit writes `r` new nodes, each an edge to one of 1,000 hubs and a
string, every node and hub of a commit in one partition, so no commit spans partitions. Medians of three 20-second runs,
interleaved, the range in parentheses; the host's 1-minute load 1.8 to 4.7, the VM's 0.9 to 2.5 before a run.

| Partitions | Requests | Connections | Commits or reads/s | Relations/s | Latency p50 / p99, ms |
|---|---|---|---|---|---|
| 1 | DirectWrite, 1 relation | 1 | 232 (224–689) | 232 | 3.15 / 15.8 |
| 1 | DirectWrite, 1 relation | 16 | 287 (205–1,123) | 287 | 56.1 / 145 |
| 1 | DirectWrite, 1 relation | 64 | 590 (250–1,334) | 590 | 90.5 / 412 |
| 1 | DirectWrite, 50 relations | 16 | 108 (99–235) | 5,392 (4,968–11,733) | 144 / 189 |
| 1 | Begin, Write, Commit, 1 relation | 1 | 184 (135–192) | 184 | 4.69 / 17.8 |
| 1 | Read-only: Begin, Read, Abort | 1 | 1,032 (1,013–1,047) | — | 0.42 / 5.3 |
| 1 | Read-only | 16 | 3,773 (3,770–4,542) | — | 3.91 / 7.3 |
| 4 | DirectWrite, 1 relation | 1 | 164 (161–230) | 164 | 5.52 / 14.1 |
| 4 | DirectWrite, 1 relation | 16 | 1,775 (1,736–2,637) | 1,775 | 5.37 / 49.5 |
| 4 | DirectWrite, 1 relation | 64 | 3,281 (476–3,324) | 3,281 | 16.6 / 57 |
| 4 | DirectWrite, 50 relations | 16 | 770 (657–830) | 38,510 (32,851–41,477) | 16.6 / 91 |
| 4 | Begin, Write, Commit, 1 relation | 1 | 244 (233–286) | 244 | 2.83 / 13.1 |
| 4 | Read-only | 16 | 7,936 (4,344–20,945) | — | 1.56 / 5.6 |
| 4 | Read-only | 64 | 11,932 (4,149–25,850) | — | 4.17 / 12.5 |

- **Through the binaries one partition commits 2 to 20% of the in-process relations of the tables above, and four
  partitions 6 to 33%.** The in-process runs offer each partition 50,000 commits a second with no client, network or
  epoch in a commit's path; a client's commit waits for its acknowledgement, 1 to 6 ms at one connection, about an epoch
  and a sync.
- **One partition stops at about 1,300 commits a second whatever the connections**, its latency growing with them, while
  no thread is busy: under 16 connections the partition's thread used 18% of a core and waited in `io_cqring_wait`, the
  frontend's threads under 2%. Four partitions take 16 connections at 5.4 ms. The commits queue in front of the
  partition, one taken per turn from the frontend's connection, each with a sync of its own (*A commit's phases*
  below).
- **The owner check takes 17 µs at the median and 25 µs at the 99th percentile** (`wal_bench --owner-checks`, the
  production owner over its logs on the VM disk), 0.3 to 1.6% of a one-connection commit; end to end, commits with and
  without a session differ by less than their spread.
- **A DirectWrite and Begin, Write and Commit differ by less than their spread**; the Begin's round trip is 0.03 to 0.09 ms.
- **Between a DirectWrite's frame and its record, a term costs 52 to 86 allocations** (`alloc_bench`, a counting global
  allocator): 6 to 10 to read the frame whatever its size, then 65 to 94 per relation in the transaction's write and 110
  to 158 in its commit, 258 for a one-relation frame of 5 terms.
- **Writes vary up to 3× between repetitions** with the length of the disk's sync, which every commit pays once (*A
  commit's phases* below). The storage process writes about 70 KB per one-relation commit, of which the log's round is
  4,096 bytes; the rest is not attributed.


#### A commit's phases through the binaries (experiment X1)

Task 12.5b's second runs, 2026-11-09, 00:10 to 00:30 UTC, on the same VM and disk, the role binaries built `-c opt` from
`b010b64f` with the phase timing below, the cluster and `client_bench` as above: DirectWrites through a session unless
noted, epochs of 2 to 10 ms. Medians of three 20-second runs, interleaved, the range in parentheses; the host's 1-minute
load 1.1 to 3.1, the VM's 0.1 to 3.9 before a run.

Each commit is timed in three places. `client_bench` times it from the call to its answer. The frontend logs `one-frame
write phases` per window of writes: its checks (the gate's tenant facts and the session's check, both asked of the owner,
the terms and the route), and its relay, from the frame leaving for the partition to the partition's answer. The
partition's core logs `commit phases`: its whole stay, from the request taken to the reply handed out, and within it the
log's writes and syncs, timed by `TimedStorage` around the log's storage, and the core's waits on its inbox. Both logs
come once a second after a run of writes, or a minute into one, at the `info` level. Times in milliseconds, means.

| Partitions | Relations, connections | Commits/s | Client | Frontend checks | Frontend relay | Partition core | Log sync | Log write | Core, rest | Queue before the core | Log share |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 1, 1 | 235 (199–619) | 4.25 (1.61–5.01) | 1.62 (0.36–2.07) | 2.54 (1.22–2.81) | 2.43 (1.17–2.67) | 2.06 (1.05–2.27) | 0.10 | 0.27 | 0.10 | 51% (48–67) |
| 1 | 1, 1, no session | 229 (135–262) | 4.37 (3.81–7.40) | 1.60 (1.33–3.25) | 2.68 (2.40–3.96) | 2.57 (2.30–3.73) | 2.21 (2.02–2.98) | 0.10 | 0.25 | 0.11 | 53% (43–55) |
| 1 | 10, 1 | 159 (153–269) | 6.28 (3.71–6.51) | 2.45 (1.20–2.67) | 3.69 (2.45–3.71) | 3.54 (2.37–3.57) | 2.08 (1.64–2.29) | 0.14 | 1.10 | 0.15 | 39% (34–46) |
| 1 | 50, 1 | 118 (113–150) | 8.41 (6.65–8.81) | 2.46 (1.53–2.98) | 5.72 (5.04–5.87) | 5.62 (4.97–5.78) | 2.01 (1.77–2.34) | 0.07 | 3.17 | 0.09 | 28% (25–28) |
| 1 | 1, 16 | 803 (438–1,125) | 19.9 (14.2–36.5) | 1.46 (0.55–3.35) | 18.4 (13.6–33.1) | 1.24 (0.89–2.28) | 1.05 (0.76–1.89) | 0.07 | 0.11 | 17.2 (12.7–30.8) | 6% (6–6) |
| 1 | 50, 16 | 147 (131–276) | 108 (58–121) | 1.11 (1.03–1.78) | 106 (57–120) | 6.79 (3.62–7.60) | 1.69 (1.55–1.89) | 0.08 | 4.82 | 99.5 (53.0–112.2) | 2% (1–3) |
| 4 | 1, 1 | 238 (155–251) | 4.19 (3.98–6.44) | 1.57 (1.48–2.94) | 2.54 (2.42–3.32) | 2.43 (2.33–3.16) | 2.11 (2.02–2.66) | 0.08 | 0.24 | 0.10 | 52% (44–53) |
| 4 | 1, 16 | 1,465 (1,362–2,527) | 10.9 (6.3–11.7) | 2.31 (0.65–3.14) | 7.69 (5.62–9.35) | 2.13 (1.37–2.51) | 1.87 (1.23–2.29) | 0.11 | 0.10 | 5.56 (4.25–6.84) | 20% (18–20) |
| 4 | 50, 16 | 863 (707–876) | 18.5 (18.2–22.6) | 2.40 (1.86–4.28) | 16.5 (15.7–18.2) | 4.05 (3.89–4.59) | 1.98 (1.95–2.46) | 0.14 | 1.97 | 12.5 (11.8–13.6) | 12% (11–12) |

(The queue before the core is the frontend's relay less the core's stay: the role connection, the storage process's
connection task and the core's inbox. The log share is the log's sync and write over the client's mean. Client to frontend
and back, the client's mean less the frontend's whole, is 0.03 to 0.19 ms in every run. The core waited on its inbox 0.00 ms
during a commit's stay in every run.)

- **Every commit has a round and a sync of its own**: one sync per commit in every run, at 16 connections and 50
  relations alike. A storage connection hands its core one request at a time and reads the next frame once the core has
  taken it, and the core takes requests between turns, each turn syncing its round, so a partition takes one request per
  turn from each connection; the frontend sends all its clients' writes for a partition over one role connection. At 16
  connections 17 ms of a 19.9 ms commit is that queue, and the partition commits one round per stay of 1.24 ms, 803 a
  second; part 1's 1,300 a second is the same bound under shorter syncs. Four partitions
  have four connections, hence four queues. This is the saturation without a busy thread of part 1. Letting a connection
  hand over several requests per turn is a design choice left open here.
- **The partition's sync is 2.0 ms whatever the round**, 4 KiB or 36 KiB, and the log's write 0.07 to 0.14 ms; at one
  connection it is half of a one-relation commit, 39% of a ten-relation one and 28% of a 50-relation one. The core's rest,
  execution, decision and apply, grows with the relations: 0.27, 1.10 and 3.17 ms.
- **The frontend's checks wait on the owner's syncs.** They are bimodal, tens of microseconds at the median of most runs
  and the length of a sync at the median of others, and the same without a session, so the gate's tenant facts alone
  meet it. With epochs of 50 to 100 ms in place of 2 to 10 ms (same build, three runs each, interleaved, host load 1.5 to
  1.8):

| Epochs | Relations | Commits/s | Client | Frontend checks | Partition core | Log sync | Log share |
|---|---|---|---|---|---|---|---|
| 2–10 ms | 1 | 254 (237–271) | 3.93 (3.69–4.22) | 1.44 (1.33–1.65) | 2.30 (2.20–2.37) | 2.03 (1.94–2.06) | 54% (51–55) |
| 50–100 ms | 1 | 371 (342–407) | 2.69 (2.45–2.92) | 0.20 (0.18–0.21) | 2.32 (2.09–2.50) | 2.04 (1.80–2.18) | 79% (77–79) |
| 2–10 ms | 10 | 110 (109–140) | 9.08 (7.10–9.11) | 4.14 (3.05–4.22) | 4.48 (3.74–4.54) | 2.72 (2.43–2.75) | 32% (32–36) |
| 50–100 ms | 10 | 263 (241–314) | 3.79 (3.17–4.13) | 0.25 (0.20–0.26) | 3.31 (2.81–3.63) | 2.21 (1.97–2.33) | 62% (60–65) |

  Each epoch close is synced in the sequencer's log on the owner's thread, and an owner ask meeting that sync waits for
  it; ten times fewer closes take 1.2 to 3.9 ms off the checks. The owner was not timed itself.
- **The spread between repetitions follows the disk.** The fastest one-connection run (619 commits a second) had syncs of
  1.05 ms against 2.06 to 2.27 ms in the others, and its owner waits shrank with them; every commit pays one sync, so the
  rate follows its length.
- **The disk's syncs.** On the VM disk at a quiet moment (VM load 0.7), 1,000 syncs each: a 4 KiB `O_DIRECT` write into a
  segment reserved by `fallocate` and `fdatasync`, the log's pattern, 1.9 to 2.3 ms at the median (p99 5 to 12 ms), the
  same into blocks written before alike; a buffered 4 KiB write and `fdatasync` 0.5 ms (p99 8 to 11 ms), and 2.6 ms
  under a host load of 3 to 6. On the host's internal SSD (APPLE SSD AP8192Z), `fsync` 20 µs and `F_FULLFSYNC` 6.2 ms
  at the median. No drive without power-loss protection is attached; whether the VM's sync reaches the host drive's cache
  flush is not known.
- **The timing costs no measurable throughput:** the same runs on the binaries without it, after the series, committed
  274 (198–380) a second at one connection and 559 (297–786) at 16, against 235 (199–619) and 803 (438–1,125) with it,
  inside each other's spread.

#### Commits batched through the binaries (batch-commits-through-the-roles)

The change `batch-commits-through-the-roles`, 2026-11-09, answers X1's one sync per commit. A storage connection hands its
partition's core every request it reads, bounded by the connection's parked bytes, and the core pushes each answer as it is
given; the commits one turn takes from the inbox share one round and one sync (`commit-rounds`, *Commits a turn takes together
share one round*); the frontend serves clients on an endpoint core per CPU. Its `notes/group-6.md` holds every run; medians of
three, the range in parentheses.

One partition, one relation a commit, a fresh cluster, the roles on one `container` machine, the frontend still on one core (6.4):

| Connections | Commits/s | p50 ms | Commits a sync | Share of in-process |
|---|---|---|---|---|
| 16 | 9,915 | 1.47 | 6.6 | 68% |
| 64 | 14,005 | 4.27 | 23.0 | 97% |
| 256 | 15,371 | 14.86 | 64.3 | 106% |

X1's 803 a second at 16 connections becomes 9,915 on that machine, and one partition reaches its in-process rate at 64
connections. Six partitions with storage alone on 6 CPUs, the frontend and compute on 6, the clients on 2, every role over
TCP (6.8, the in-process rate `wal_bench` on the storage machine):

| Relations | Connections | Commits/s | Relations/s | p50 ms | Commits a sync | Share of in-process |
|---|---|---|---|---|---|---|
| 1 | 256 | 32,658 (32,362–33,670) | 32,658 | 6.7 | 13.7 | 38% |
| 1 | 1,024 | 41,515 (40,306–41,865) | 41,515 | 14.9 | 28.8 | 48% |
| 10 | 256 | 9,939 (9,863–10,027) | 99,390 | 18.0 | 8.8 | 86% |
| 10 | 1,024 | 10,679 (10,567–10,882) | 106,790 | 62.8 | 22.8 | 93% |

On the same layout a single client commits 411 (396–448) a second, p50 2.24 ms, p90 3.42 ms, p99 4.96 ms: about 1.0 ms in
storage (the log's write 0.21 ms and sync 0.35 ms), 0.6 ms between the frontend and storage, 0.5 ms between the client and
the frontend, 0.1 ms in the frontend's checks.

- **At one relation a commit the rest of the gap is storage's.** No role is busy: a partition's core alternates its own
  work, about 2.3 ms a round of 29 commits, with its log's write and sync, about 1.3 ms. 24.5% of storage's samples load
  and checksum a store block for a term's lookup (xxhash), which the in-process rate pays as well.
- **The core's wait in its sync is what fills a round.** Executing the next round while one syncs ran 7 to 17% below
  inline syncs at every point: a sync's commits halved, its syncs doubled, each slower (0.7 to 1.8 ms). It was reverted.

#### Turns, pins, bound walks, the frontend's memory and readers' aborts through the binaries

Task 12.5b's third runs, 2026-11-09, 00:56 to 02:17 UTC, on the same VM and disk, with part 2's role binaries and the
`dev` profile as above; medians of three runs, interleaved, the range in parentheses; the host's 1-minute load 1.6 to 5.1,
above 2 for most of the series. The full tables are in the change's group-12 notes.

| What | Measured | Against |
|---|---|---|
| A one-relation commit's queue before the core, beside Scans of `?s <w> ?o` over lists in the 64 MiB posting cache | 0.10 ms alone; 1.28 (1.21–1.31) beside 1 Scan, 3.06 (2.94–3.14) beside 4, 2.92 (2.68–3.04) beside 16; p99 12 → 20 ms | `TURN_BYTES`' estimate, 130 to 260 µs a turn |
| The same over lists past that cache (367,600 nodes) | 34.8 ms beside 1 Scan (p99 297 ms), 272 ms beside 4 (p99 0.9 to 1.1 s) | the same |
| A Pin Scan, snapshot level against monotonic-fast | 5 to 20 µs of storage CPU per partition per read; at 16 partitions and 16 connections 13% fewer reads, 1.1 ms slower | 16 ranges, pins at once |
| `sweep_bound`, and `bound_answered`'s walk | 4 to 12 ns per open transaction: 4 to 7 µs at 1,000 open, 37 µs at 10,000, 1.2 ms at 100,000; two such walks a turn, `resend_bound` the second | — |
| The frontend's memory per stalled client connection | 5.6 to 5.9 MiB at 16, 64 and 256 connections; 1.0 to 1.1 MiB with nothing held | `held_bytes` and one frame, 4.08 MiB |
| `<hot> ?p ?o` readers aborted beside a writer under an existing predicate, about 200 commits a second | every row: 99%, with or without the list's rewrite; one row: 97% with it, 18% without it | — |

- **A turn's length is its store reads, which `TurnBudget` does not count.** A profile of the partition's thread over a
  cold Scan puts 89% in fjall loading and checksumming a block per list (`Lists::read_store` opens a range per key); with
  lists cached the Scan reads 400,000 rows a second, past the cache 11,000 to 40,000. One frame per Scan per turn holds:
  16 Scans add no more to a commit's wait than 4.
- **A pin's cost is per partition per read**, 22% of a 16-partition read's storage CPU; one partition's read pays it once.
- **The walks are free below a few thousand open transactions** and cost a commit as much as its own work at ten
  thousand.
- **The bound per client connection holds**, its memory returned within 12 s of the partition continuing; the frontend's
  worst case is connections × (`held_bytes` + `max_frame` + about 1.5 MiB), about 26 GiB at 4,096 connections, not 16.
- **The predicates' list's rewrite costs only readers that stop before the written predicate**: a reader of every row
  conflicts on the written forward list anyway; a reader stopped at one row conflicts on the list alone (A/B against a
  storage binary that leaves the list alone when it names the predicate).

## The model: one partition per core

**References.** ScyllaDB runs one shard per core on Seastar: each shard owns a share of the data and its own memory,
and shards exchange messages over per-pair queues, never shared structures. TigerBeetle runs each replica
single-threaded on one core, batching thousands of operations per request through io_uring. Both put the serial
section of a write on one core that owns its data, and scale by adding owners.

**Shape.** A partition is a replica group: its own VSR log and commit step over the buckets the placement map gives
it, with a replica on each of three nodes. Each core of a storage node hosts one replica of three partitions, leading
one and following two, and keeps one LSM instance holding a keyspace per hosted replica (P8, P11). Each core's compio
runtime owns its I/O queues, as the distribution note's runtime model states; with partitions, the commit step is per
core too.

**What the model changes in the clustering architecture.**

1. **Placement follows the bucket.** Predicate sharding puts a hot predicate on one partition and so on one core:
   about 40,000 relations per second for that predicate, whatever the cluster's size. Placement by the bucket of the
   subject's ID spreads a predicate across partitions. An edge's forward entry then lives with its subject and its
   reverse entry with its object, usually elsewhere; the reverse follows by change stream, the mechanism the
   distribution note plans for tiles, needed from the first partitioned stage. *Superseded by P24 (2026-11-02): both
   entries stay in the subject's partition, and the object's home keeps a referrer set naming where to look.*
2. **The term dictionary is a partitioned unique index.** An IRI's uniqueness is checked by the partition owning its
   hash. Deriving an entity's bucket from the same hash, of its IRI or of its UGS `@key`, keeps creation and the check
   in one partition (P1).
3. **Replica groups multiply**: with three replicas per core, a cluster runs as many partitions as it has storage
   cores, and replication runs core to core, one connection per core per peer node (P8). Idle groups quiesce and
   heartbeats coalesce, as multi-Raft systems found necessary; closed epochs reach the sequencer once per node (P12).
4. **A transaction spanning partitions on one node takes the cross-group path**, Calvin sequencing and epochs over
   shared-memory queues. The parity rule holds: stage 8's single container runs the paths a cluster runs. D3's
   guarantees apply per partition: strict serializability within one, serializability at a closed epoch across them.
5. **Each core has its own LSM instance**, shared by the replicas it hosts. A journal shared across cores would
   serialize the partitions again; within a core one thread drives every replica, so sharing costs nothing. fjall's
   workers and the memory of memtables and caches take a budget per node (P11).
6. **Rebalancing is one mechanism**: a bucket moving between cores is the operation that moves it between nodes.

The three roles stay as they are. Stage 5.5 settles the identity (D1 revised by P7), the log format (P9, P10) and the
metadata group (D4, more partitions to place); stage 8 builds the partitions, the reverse entries by change stream and
the replica placement. *P24 (2026-11-02) replaces the reverse entries by change stream with referrer sets.*

### Large nodes

Estimates for a three-node cluster, extrapolated from the 12-vCPU measurements at full efficiency, since the
development VM is no production target; nothing here was measured on a large machine, and the 1–3 µs per store
operation comes from one workload. A node runs a partition core per core less one per eight, rounded up (P4); each core hosts three replicas, so
the cluster runs three partitions per partition core of a node. A leader costs a partition core what one pipeline
costs, about 38,000 relations per second; a follower with today's physical log only logs and applies, assumed (not
measured) to cost 45% of that, so a core leading one partition and following two carries about 20,000 relations per second per partition.

| Cores per node | Partition cores | Partitions | Cluster ceiling |
| --- | --- | --- | --- |
| 12 | 10 | 30 | 0.6M relations/s |
| 48 | 42 | 126 | 2.5M |
| 64 | 56 | 168 | 3.4M |
| 128 | 112 | 336 | 6.7M |

One partition per core index, its leader on one node and its followers idle half the time on the others, would reach
about 4.3M on 128-core nodes; spreading leadership is worth about half again.

What each node then writes and sends, at the ceiling:

| Cores per node | Disk before compaction, physical log and fjall journal (1.9 KB per relation) | The same, logical log without fjall's journal (about 0.95 KB) | Replication out, physical (2 × 1.1 KB) | Replication out, logical (2 × about 150 B) |
| --- | --- | --- | --- | --- |
| 12 | 1.1 GB/s | 0.6 GB/s | 3.5 Gbit/s | 0.5 Gbit/s |
| 48 | 4.8 GB/s | 2.4 GB/s | 15 Gbit/s | 2 Gbit/s |
| 64 | 6.4 GB/s | 3.2 GB/s | 20 Gbit/s | 2.7 Gbit/s |
| 128 | 12.7 GB/s | 6.4 GB/s | 39 Gbit/s | 5.4 Gbit/s |

Every node holds a replica of every partition in a three-node cluster, so each writes at the cluster's rate. LSM
compaction multiplies the store's share, about 0.8 KB per relation, by a further 5–10×, assumed until measured. At 48 cores and above, the
disk's bandwidth and its flush rate bind before the cores, then the network; the logical log halves the first and
divides the second by seven.

### Reads

Reads take no commit step and write nothing, so they scale with every replica, not with leaders alone. Query
classification (UGS note) decides which replicas answer and how long a read waits; it does not change a read's CPU.

| Read level | Answered by | Wait |
| --- | --- | --- |
| Latest state of one partition (key lookups, colocated trees) | The leader, or a replica caught up to its latest commit | None |
| Monotonic-fast (monotonic queries) | Any replica | None |
| Snapshot (non-monotonic, across partitions) | Any replica that has closed the epoch | Up to one epoch |

Every partition core leads one partition (P8), so reads of a partition's latest state still spread over every core.
Estimates for 25 nodes of 48 cores (1,050 partitions), unmeasured: about 35–70 million point lookups a second while the
data sits in memtables and caches, and 10–25 million once reads go to SSD; about 1.5–2 million reads a second of an
entity's 100 neighbours with one property each, which reach about 95 partitions unless `@colocate` keeps them together.
A read of a versioned key from the store's tables costs a seek per level until prefix filters exist (P19), so the
figures above hold for cached data. Whole-graph analytics are bound by the network between partitions rather than by
read speed; the tile stage's two-dimensional partitioning is the structural answer, and compute-heavy algorithms
(graph neural networks, embeddings) belong on GPUs in the compute role.

## Decisions for stage 5.5

Twelve decisions the partitioned model needs before stage 5.5 specifies identity, the log and the metadata group.
P1–P6 follow from the model, P7–P12 from scaling it to large nodes. Each states the trade-off, a recommendation and its
consequences. The user adopted all twelve on 2026-11-01, as refined by P13–P20 and amended by P24.

### P1. Where an entity's bucket comes from

**Trade-off.** The bucket is part of every system ID, so it is fixed when the entity is created and decides its
partition until the bucket moves.

- *A hash of the business identity* (the canonical `@key` tuple, or the IRI of data without a schema) puts an entity
  where its identity's uniqueness is checked. Creation and the dictionary's check share one partition, and any node
  finds an entity's partition from its IRI alone, with no directory. The price is locality: related entities land on
  unrelated partitions, so a transaction touching several crosses partitions, more of them the larger the cluster
  (P12).
- *The creating partition's own bucket* keeps every creation local and places related entities together. The
  dictionary entry still has to live where its IRI hashes, or uniqueness cannot be checked, so every creation becomes
  a write to two partitions.
- *An entity without a key* has no identity whose uniqueness needs checking: its ID is unique by allocation. Hashing
  its handle would scatter it for no gain.

**Recommendation.**

- A keyed entity's bucket is XXH3-64 (the repository's cross-language hash) of its key's canonical encoding, reduced
  to the bucket's 24 bits (P7); P15 and P16 say what the hash covers. Keys are immutable in UGS, so the bucket never follows a key change.
- An entity or relation instance under `@colocate` takes the bucket of the participant it is colocated with.
- An entity without a key takes the bucket of the entity it is created with, and otherwise a bucket of the partition
  creating it.
- Data without a schema hashes the IRI.
- The mapping from an IRI to its ID never changes once written, since keys are immutable and IDs never reused, so any
  partition caches another's dictionary entries without invalidation. *Narrowed by P24 (2026-11-02): the ID-to-IRI
  direction never changes, since IDs are never reused. In stage 5.5 an IRI-to-ID entry never changes either, since
  deletes are by pattern and no entity is deleted, so the cache stays valid. From stage 6 an entity delete and a
  re-created IRI change it, and the home's "deleted" answer and the delete's fence catch a stale entry; stage 8's
  reclamation removes entries with no delete before them, and stage 8 specifies how a registration naming a reclaimed ID
  is answered before it builds reclamation.*

**Consequences.**

- The canonical key encoding and the hash are frozen in a specification, with a version, since every stored ID
  depends on them.
- The dictionary is keyed by the same hash, so a lookup of an IRI goes to one known partition, and a repeated lookup,
  such as a hub's, is answered by the asking core's cache.
- Locality comes from `@colocate`, which the schema already declares, not from where a write arrives.

*Amended by P24 (2026-11-02): a partition's first reference to an entity it does not own registers at the entity's
home, which gets or creates its ID; the dictionary is naming state, and existence is read from committed data.*

### P2. How buckets are assigned to partitions

**Trade-off.**

- The bucket field fixes the number of buckets for good, since IDs carry them: 16.7 million with P7's 24 bits.
- What stays open is how they are assigned: many buckets per partition make each move small and let load even out
  finely; few make the placement map small and a move coarse.
- Couchbase fixes 1,024 vBuckets for the same reason: a fixed count, moved whole.

**Recommendation.**

- Every bucket exists from the start.
- The placement map assigns them to partitions as contiguous ranges, split evenly at cluster creation.
- Rebalancing moves ranges between partitions, on one node or across nodes.
- The 30 partitions of three 12-core nodes hold about 560,000 buckets each; the 336 of three 128-core nodes about
  50,000; a thousand 128-core nodes still about 150.

**Consequences.**

- The placement map is a short list of ranges, whatever the bucket count.
- A single hot entity is bounded by its partition. Hubs remain the tile stage's problem.
- Keys stay predicate-major for scans: a bucket's data is not one contiguous key range, so a move scans its source
  partition and filters by bucket. That is acceptable for a rare operation; the 128-bit key format should not reorder
  keys for it.

### P3. Which writes stay in one partition

**Trade-off.**

- Entity-centred placement makes everything about one entity local.
- Relationships between entities, and every transaction naming several root entities, cross partitions. With hashed
  buckets a transaction touching `m` root entities reaches about `min(m, partitions)` of them: a 50-root commit on
  three 12-core nodes reaches about 25 partitions, on three 128-core nodes about 46.
- A cross-partition transaction goes through the sequencer (Calvin ordering, no two-phase commit). That costs up to an
  epoch of latency and work in every partition it reaches.
- The distribution note's classification already sorts invariants into coordination-free, segmented and coordinated;
  per partition it applies unchanged, with the partition in the group's place.

**Recommendation.**

- **Single partition:**
  - a keyed entity's creation;
  - every property write on one entity;
  - an edge or relation instance, stored with its subject's (or its `@colocate` participant's) bucket, with its
    reverse entry and the object's indexes following by change stream;
  - a cardinality constraint on the stored end;
  - everything inside a `@colocate(root)` tree.
- **Cross partition (sequenced):**
  - a transaction writing root entities in more than one partition whose writes include a check that can fail (a new
    key's uniqueness, a constraint) or whose reads must be validated (P14);
  - a secondary `@unique` value, whose index is partitioned by the value's hash;
  - a cardinality constraint on both ends of a relation;
  - a `restrict` delete across partitions;
  - an `@acyclic` edge between trees.
- **Across partitions without the sequencer:** a transaction of blind, coordination-free writes (no reads, no check that
  can fail) applies its share in every partition stamped with one epoch, and the epoch closes only once every share is
  durable, so a snapshot at that epoch sees all of it or none, as in Bailis's read-atomic multi-partition transactions.
- A relation without `@colocate` is stored with its first role's participant, replacing UGS's default of placing it
  by predicate.
- Bulk loads take their own path, specified in stage 5.5 as a requirement: the loader partitions its input by bucket
  and each partition ingests its share as single-partition commits, with a watermark marking the load complete. The
  benchmark's workload, 50 new root entities per commit, would otherwise reach dozens of partitions on every commit.
- Cross-partition work reaches each partition as one batch per epoch (P12).

**Consequences.**

- UGS's placement rules change from predicate to participant, and the coordination report counts cross-partition
  writes per type.
- Snapshot reads of reverse entries wait for the change stream's watermark, as the epoch design provides.
- The bulk loader becomes a stage-8 deliverable beside the partitions.

*Amended by P24 (2026-11-02): an edge's reverse entry and the object's indexes stay in the subject's partition, with no
change stream; blind writes across partitions are sequenced, with no validation exchange.*

### P4. How many cores a node gives to partitions

**Trade-off.**

- Every core hosting replicas maximises commit threads. Side by side, total throughput still rose from 6 pipelines to
  12 (201,654 to 263,196 relations/s at 50 per commit).
- But each pipeline lost 41–47% at 12, because fjall's flush and compaction threads, the network and, in the single
  container, the frontend and compute roles compete with the commit threads.
- ScyllaDB avoids that by running compaction inside each shard under a CPU scheduler; fjall runs its own threads,
  outside any compio runtime.
- Reserving cores lowers the peak and makes each partition's latency predictable.
- Machines of 48 cores and more have several sockets or chiplets; memory touched by a thread that later moves to
  another socket is read across the interconnect.
- The VM runs are an upper bound on contention: the host has 4 efficiency cores and its own processes.

**Recommendation.**

- A node gives every core but one per eight, rounded up, to partitions (10 of 12, 42 of 48, 56 of 64, 112 of 128);
  the reserve runs fjall's workers and the other roles' I/O.
- Each partition core is pinned, hosts three replicas (P8) and never migrates while running, so mimalloc's per-thread
  heaps keep its memory on its socket.
- The count is a node setting with that default, since the single container and a dedicated storage node differ.
- Changing it rebalances buckets through P2's mechanism.
- Stage 8 measures the default on bare metal of 64 cores or more before it is final.

**Consequences.**

- The placement map tolerates partition cores joining and leaving a node.
- fjall's worker count and the store's memory are set per node from the reserve and the budget (P11).

### P5. How `wal_bench` measures the model in one process

**Trade-off.**

- Separate processes, as measured, are cheap and isolate partitions perfectly. But they miss what partitions share in
  one process: the allocator, the store's worker budget, cross-partition queues, the cache hierarchy, a device's
  flushes.
- An in-process benchmark needs the partition runtime itself, which stage 5.5 builds.
- One shared generator thread already produces up to about 50,000 commits/s. Twelve partitions at one relation per
  commit would need over 100,000, so a shared generator becomes the bottleneck it is meant to measure around.
- Cross-partition commits cannot run before the sequencer exists, nor follower load before replication.

**Recommendation.** Three steps.

1. In stage 5.5: N partition cores in one process, each with its own generator seeded from the run's seed and
   producing only its own buckets' commits, on the logical log with shared device flushes (P9, P10), measured on the
   VM disk and on a drive without power-loss protection.
2. With replication in stage 8: each core leading one partition and following two (P8), with followers fed from the
   leaders' logs (the distribution protocol's X4).
3. With the sequencer in stage 8: a chosen share of cross-partition commits, drawn by the same seeded generators.

The report gives per-partition and total throughput, disk bytes and flushes per second, as the side-by-side table
does, so the steps are comparable. Stage 8 runs it on a 64-core bare-metal machine.

**Consequences.**

- The benchmark becomes the acceptance measure of P4's default, P10's flush sharing and P3's cross-partition cost.
- The multi-process figures stay in this note as the baseline.

### P6. Where a partition applies its rounds

**Trade-off.**

- Applying on a second core, pipelined with the next round, takes the apply's 37% off the commit thread: up to 1.6×
  for that partition, using two cores.
- Two partitions on those two cores give nearly 2× (1.97× side by side at two pipelines) for every workload that
  spreads across buckets.
- With three replicas per core (P8), a core already carries one leader's work and two followers' applies; a second
  apply core per leader would break that balance.
- Pipelining also lets the store lag the commit point, so a read after a commit waits for the apply. The log is the
  durability point, but read-your-writes then needs that wait.
- It pays only when one partition's load cannot be spread: a single hot bucket, which P2 cannot split.

**Recommendation.**

- Every replica applies on its core, the leader in the round after validation and logging, a follower as its log
  arrives.
- Pipelining is kept as a later option for hot partitions, taken up only if stage 8's measurements show a single
  bucket exceeding one core.

**Consequences.**

- A core stays one thread, as in TigerBeetle, driving its three replicas.
- The parallel apply by key range in backlog item 11's earlier plan gives way to more partitions.

### P7. The ID's bucket width and its sequences (revises D1)

**Trade-off.**

- D1 gives the bucket 16 bits: 65,536 buckets. Three nodes use few of them, but 128-core nodes, 112 partitions each,
  reach 65,536 partitions at about 585 nodes, and moves grow coarse long before. The bucket is in every ID, so
  its width can never grow later.
- D1 also numbers IDs per (type, bucket). With hashed buckets a type's entities touch every bucket quickly, so leasing
  sequence ranges per (type, bucket) means a lease for nearly every bucket of every type: millions of round trips to
  the metadata group.
- Unique sequences per type are unique within every bucket too, and survive a bucket's move to another partition,
  where a counter per partition could reissue an ID the old partition had issued.

**Recommendation.**

- The bucket grows to 24 bits by taking 8 of the 32 reserved: a 16-bit type prefix, a 24-bit bucket, a 64-bit
  sequence and 24 reserved bits.
- A sequence is unique per type across the cluster, leased by the metadata group to partitions in large blocks per
  (type, partition).

**Consequences.**

- D1's layout and its per (type, bucket) counters are revised before stage 5.5 builds on them; the term dictionary
  allocates from per (type, partition) blocks.
- Leases are as many as types times partitions, each covering many creations.
- 24 bits stay reserved, room for the tile coordinate D1 named as a candidate.

*Field order revised by P24 rule 1 (2026-11-02): type, sequence, bucket, reserved.*

### P8. Where a partition's replicas run

**Trade-off.**

- Placing partition `i` on core `i` of each of three nodes keeps replication core to core, but its leader's core does
  a leader's work while the two follower cores, logging and applying only, idle half the time.
- Hosting several replicas on each core, with leadership spread, balances the work, as multi-Raft systems spread
  their leaders. Each core then talks to more peers unless the placement keeps them aligned.
- After a node fails, its led partitions elect leaders among their followers, and those cores carry more than their
  share until the node returns.
- Nodes with different core counts break any alignment by index.

**Recommendation.**

- With three replicas, core `i` of every node hosts three partitions, one led from each node, all three replicated
  among the cores `i` of the three nodes; in larger clusters, each core's three replicas pair with the same core index
  on their peer nodes.
- Each partition has a preferred leader, spread so that every core leads one; leadership returns to it after a
  failure heals.
- The replication factor is a setting: three in a cluster, one in the development container, where the alignment across
  nodes does not apply.
- Storage nodes in one cluster have the same number of partition cores; a node that differs forwards between cores,
  as a fallback, not a design point.

**Consequences.**

- A cluster runs as many partitions as it has partition cores, about half again what one partition per core index
  carries (*Large nodes*).
- Replication needs one connection per core per peer node, and no message crosses cores on its way in.
- The placement map records preferred leaders; the metadata group restores them.

### P9. What the log records

**Trade-off.**

- Today's log carries physical key deltas, about 1.1 KB per relation, and fjall journals the same writes again,
  about 0.8 KB: two write streams and two flushes per round.
- A logical record, the resolved quads with their IDs and the dictionary entries the round creates, is about 100–200
  bytes. Disk writes before compaction halve, replication bytes fall about sevenfold, and with the log as the only
  journal fjall's own journal goes (the design's option C).
- Followers and recovery then derive the index keys from the quads themselves, so a follower does part of the
  execution as well as the apply, and the derivation must give identical keys on every replica and every version
  that replays it.

**Recommendation.**

- The log records resolved quads with their IDs, the round's new dictionary entries and its order of application;
  each replica derives the keys deterministically, with the derivation's version in the record.
- fjall runs without its journal: the store records the log position it has applied, and recovery replays the log
  from there. P18 revises this: fjall 3.1.13 has no journal-less mode.

**Consequences.**

- Stage 5.5 replaces stage 5's physical-delta log format.
- A follower's work rises from about 45% of a leader's towards 55–60%, which P8's balanced leadership absorbs.
- Mixed-version replay checks the derivation version, which the rolling-upgrade rules in the distribution note cover.

### P10. How partitions sharing a device flush

**Trade-off.**

- Without power-loss protection, which no deployment may be assumed to have, a flush empties the drive's whole
  volatile cache. A hundred partitions each flushing their own log every round saturate a drive's flush rate long
  before its bandwidth.
- One log file per device, shared by its partitions, makes one flush cover every round ready at that moment, at the
  cost of an atomic offset reservation per append and recovery separating the partitions' streams.
- With power-loss protection a flush is cheap, and separate files per partition are simpler.

**Recommendation.**

- The log interface gives each partition its own logical stream: an append returns a durability ticket, which a flush
  completes.
- Partitions on one device share a physical segment, appending at reserved offsets, and one flusher per device
  completes every ticket written before its flush.
- Separate files per partition stay the alternative; P5's benchmark chooses on a drive without power-loss protection.

**Consequences.**

- Shared-nothing holds everywhere except one atomic counter and one flusher per device.
- Recovery reads each device's segments once and hands every partition its stream.
- A shared segment is truncated only once every partition writing it has passed it, so a lagging partition holds the
  device's segments; the flusher reports which partition holds them.

### P11. The node's budget for the store

**Trade-off.**

- An LSM instance brings its own worker threads and memtables. One instance per replica on a 128-core node would be
  336 instances: hundreds of threads and, at fjall's 64 MiB memtables, about 40 GB of memtables before any cache.
- Shrinking memtables to fit costs more than it saves: 8 MiB cost 31% to flush pressure.
- One instance per core serves that core's three replicas as keyspaces; one thread drives them all, so sharing costs
  nothing there, and without fjall's journal (P9) no journal serializes them.

**Recommendation.**

- One LSM instance per partition core, a keyspace per hosted replica.
- The node divides one budget among the instances: fjall's flush and compaction threads, capped per instance and
  together sized to P4's reserve, and memory for memtables and caches, the dictionary cache included.
- Before stage 8, check whether fjall can share one worker pool across instances or run its background work on the
  partition's runtime, ScyllaDB's way; the budget applies either way.

**Consequences.**

- A 128-core node runs 112 instances, not 336.
- The dictionary cache's 64 MiB becomes a share of the node's budget, per core.
- A replica catches up from a checkpoint of its own keyspace, which the shared instance provides per keyspace.

### P12. How partitions meet the sequencer

**Trade-off.**

- Hundreds of partitions closing an epoch every few milliseconds would send the sequencer tens of thousands of
  messages a second, and a cross-partition transaction sent to each partition separately multiplies that by its
  fan-out.
- Aggregating per node adds one hop inside the node, a shared-memory queue.

**Recommendation.**

- One epoch agent per node closes an epoch for all its partitions in one message, and receives the sequencer's
  boundaries once.
- The sequencer delivers cross-partition work to each partition as one batch per epoch, never per transaction.
- Change streams between partitions travel the same way: one batch per pair of nodes per epoch, never per pair of
  partitions, which at 25 nodes of 48 cores would be over a million pairs.

**Consequences.**

- The sequencer's load grows with nodes, not cores.
- An epoch closes on a node once its slowest partition has applied up to it, so one lagging partition delays the
  node's close; the agent reports which.

*Amended by P24 (2026-11-02): no change streams run between partitions.*

## Decisions from the review before drafting

A critical review of this note, `../notes/perf-optimization.md` and `review-stages-5.5-and-6.md` against the repository found
eight more questions stage 5.5 has to answer before its specifications are drafted. P13–P20 state them in the format of
P1–P12; they refine P1, P3 and P9 where they say so. The user adopted all eight on 2026-11-01. The ninth, musl or
glibc, is D7 in the review note, adopted the same day.

### P13. What a version means across partitions

**Trade-off.**

- Stage 5 numbers commits per commit step: `max(previous + 2, 2 × clock µs)`, even and increasing. With a commit step
  per partition, every partition runs its own version clock, and the stored key carries that version.
- *Clock versions per partition* keep stage 5's sequencer. A snapshot at epoch `e` then needs each partition's version
  at the close of `e`: every partition keeps a table from epoch to version, and every read at an epoch looks it up.
- *Versions built from the epoch*, the epoch in the high bits and a sequence within it in the low bits, make a
  snapshot at epoch `e` one bound on the version in every partition, with no table. A partition must know the current
  epoch when it commits, which the distribution note already requires (every commit is stamped with an epoch), and the
  sequence must not run out while an epoch stays open, as it may while the sequencer changes view.

**Recommendation.**

- A version is 64 bits: a 42-bit epoch and a 22-bit sequence within it, even as stage 5 keeps versions. At epochs of
  1 ms and longer, 42 bits last over a century; 22 bits allow two million commits per partition per epoch.
- A partition stamps its commits with its open epoch: one above the latest close delivered to it, or its own epoch
  when it has advanced further, so no commit is stamped with a closed epoch and a snapshot at a closed epoch never
  changes. A partition whose sequence runs out in its open epoch advances its epoch on its own, as a logical clock may;
  the sequencer only closes epochs. A partition has closed an epoch once it holds that epoch's close and has applied
  every round stamped at or below it, and a snapshot read waits for that in every partition it asks.
- `as of time t` maps to an epoch by the wall-clock time recorded at its close, as the distribution note states.

**Consequences.**

- Stage 5's clock-following sequencer gives way to epoch-based versions; reads of a partition's latest state use its
  latest version, with no epoch wait.
- Rollups write below deltas as before, at any free version below them.
- The `transactions` and `commit-rounds` specs are restated with these versions in stage 5.5.

*Revised by P24 rule 4 (2026-11-02): a partition stamps exactly one above its last delivered close and never advances on
its own; when its sequence would run out, it asks the sequencer to close the epoch.*

### P14. Interactive transactions across partitions

**Trade-off.**

- UQL makes transactions within one group interactive and transactions across groups submitted whole. With hashed
  buckets nearly every transaction naming several entities spans partitions, so interactive transactions would remain
  only for one entity or one colocated tree; the stage-5 protocol and the Rust client offer begin, read, write and
  commit for every transaction.
- *Refusing* them is honest and simple, but pushes every multi-entity application onto `@colocate` or onto
  transactions submitted whole.
- *Two-phase commit* at a coordinator is ruled out by the architecture: no two-phase commit, no distributed deadlocks.
- *Optimistic commit through the sequencer*: the transaction reads at a closed epoch and buffers its writes; its commit
  carries its read set, each key with the version it read, and its writes to the sequencer. Every partition it reaches
  validates its own reads deterministically, the partitions exchange their outcomes as Calvin's participants exchange
  reads, and all commit or all abort. It costs an epoch and one exchange, and aborts and retries under contention.

**Recommendation.**

- Interactive transactions within one partition commit through its commit step, strictly serializable, as today.
- Interactive transactions across partitions commit optimistically through the sequencer, serializable at their epoch
  (D3), within the 10-second limit on read-write transactions.
- A transaction with an empty read set whose writes are all coordination-free (P3's blind writes) skips the validation
  exchange.

**Consequences.**

- UQL's rule reads: interactive everywhere, optimistic across partitions; the UGS note changes with it.
- The binary protocol keeps one transaction shape; the response names an abort's conflicting key.

### P15. What the bucket hash covers (refines P1)

**Trade-off.**

- A key is unique across its declaring type and every subtype, so the hash must give a subtype's instances the
  bucket of the type that declares the key, or the uniqueness check spans partitions.
- The IRI template `{namespace}{Type}/{key}` leaves open whether `Type` is the declaring or the concrete type, and an
  `@iri` template need not let the key be read back from an IRI, so a node holding only an IRI could not find its
  bucket.
- Stage 5.5 comes before UGS keys (stage 6). Whatever it hashes in the meantime fixes buckets in stored IDs; with no
  outside users a change in stage 6 rewrites data, but avoiding it costs nothing.

**Recommendation.**

- The bucket hashes the type prefix of the type that declares the key, then the key's canonical encoding (P16 adds the
  namespace).
- IRIs are minted from the declaring type, `{namespace}{DeclaringType}/{key}`, and the compiler refuses an `@iri`
  template from which the key cannot be read back.
- Data without a key, in stage 5.5 and in `@open` types after it, takes its IRI as the key of a built-in `Resource`
  type, so stage 6 adds keyed types without moving existing data.

**Consequences.**

- Any node finds an entity's bucket from its IRI and the catalog.
- The UGS note's instance-IRI rule names the declaring type.

### P16. Tenants in the bucket

**Trade-off.**

- IDs carry no tenant, and hashing across all 16.7 million buckets spreads every tenant over every partition:
  a small tenant's queries scatter over the cluster, dropping a tenant touches every partition, and tenants share
  every partition's load.
- Confining a tenant to fewer partitions gives locality and isolation, and must still let a large tenant spread
  as it grows, without changing an ID.

**Recommendation.**

- A bucket's 24 bits are 8 bits hashed from the tenant and 16 bits hashed from the key (P15): each tenant owns an
  aligned run of 65,536 contiguous buckets, shared with the tenants whose hash it shares.
- The placement map assigns runs to partitions like any range (P2): a small tenant starts on one partition, and a
  growing one is spread by splitting its run, up to 65,536 partitions, with no ID changing.
- Initial placement spreads the runs of the tenants that exist, so a single-tenant cluster spreads its one tenant
  over every partition.

**Consequences.**

- A tenant's queries reach only its partitions; a tenant drop reaches only them.
- The rebalancer watches load per run as well as per partition.

### P17. Reverse entries of heavily referenced nodes

**Trade-off.**

- Under bucket placement every reverse entry pointing at a node goes to that node's partition. UGS's shared time and
  space nodes (days, cells) and widely used concepts receive edges from the whole cluster, so one partition would
  absorb the cluster's ingest for them.
- *Reverse entries with the object* keep inbound reads in one partition, at the price of that hotspot.
- *Reverse entries with the subject*, as an index by object in the subject's partition, keep every write local, but an
  inbound read then asks every partition: over a thousand messages per read.
- *Splitting a hot node's reverse list* across a few partitions by the source's bucket, as PowerGraph cuts high-degree
  vertices, spreads the ingest and keeps inbound reads to a few partitions.
- D2's type prefixes already make `rdf:type` a range of IDs, so type membership needs no reverse list; the key format
  already has split parts.

**Recommendation.**

- Reverse entries live with the object, maintained by change stream.
- A node whose type the schema declares shared, or whose reverse list passes a size threshold, has its reverse list
  split into `k` parts by source-bucket range, each in a different partition; the placement map records the split
  nodes and their `k`, and inbound reads ask those `k` partitions.
- `rdf:type` is answered from type-prefix ranges and never stored as a reverse list.

**Consequences.**

- The 128-bit key format gives reverse keys a source-bucket-range part, using the split parts it already has.
- The tile stage generalises the split to every list.

*Superseded by P24 (2026-11-02): reverse entries stay with their subjects, so a hub's inbound entries are spread over
its referrers' partitions by construction; reverse keys carry no source-bucket part and the placement map records no
split nodes. `rdf:type` is still answered from type-prefix ranges.*

### P18. The log as the only synced stream (refines P9)

**Trade-off.**

- P9 has fjall run without its journal. fjall 3.1.13 offers no such mode, only `manual_journal_persist`, which still
  writes the journal but leaves syncing it to the caller; the archived stage-5 design names a fork as the only way to
  drop it.
- *A fork* removes the journal's bytes, about 0.8 KB per relation, and costs maintaining the fork against fjall, which
  is pinned already because `Store::compact` calls its hidden functions.
- *Manual persistence* keeps the journal's bytes in the page cache and on disk, but never syncs it: the log is the only
  synced stream. After a crash fjall recovers a prefix of its batches, and the store's applied position, written in the
  same batch as its data, says where log replay resumes.

**Recommendation.**

- fjall runs with `manual_journal_persist`, its journal never synced; the log is the only stream synced, and recovery
  replays the log from the store's applied position.
- A fork is taken only if P5's benchmark shows the journal's bandwidth binding on large nodes.
- A log record carries the resolved quads with their IDs, the round's new dictionary entries, its version (P13), the
  catalog version it was validated against and the derivation version.
- Rollups stay local to each replica and out of the log: replicas agree on contents, not on bytes, so a replica catches
  up from a checkpoint, never by comparing files.

**Consequences.**

- P9's gain becomes one synced stream instead of two at once, and half the disk bytes only with the fork.
- The rolling-upgrade rules check the derivation version and the catalog version on replay.

*Amended by P24 (2026-11-02): the record also carries each transaction's registrations as a referrer (entity ID, bucket
group, the reply's generation), entries copied in both directions among the new dictionary entries, drops as operations,
deletes by pattern as their removals, session-table entries, index-build records, and the registrations a home accepts
with the referrer-set members they add or renew.*

### P19. Reads of versioned keys (carries `../notes/perf-optimization.md` forward)

**Trade-off.**

- lsm-tree's bloom filters hash the whole stored key and serve point gets only. A read of a versioned key is a range
  read, which consults no filter and seeks into every table whose key range covers it: one seek per level for a key
  held in one level, and all of them for an absent key.
- `../notes/perf-optimization.md` measured absent keys at 34% of stage 2's apply thread. The stage-5 pipeline avoids them for
  new IDs, whose lists it never reads (`wal_bench` counted none), but reads of existing keys outside the cache, and
  every query, pay the seeks.
- *Existence markers* (that note's option A) prove absence with a point get, at one more key per list.
- *Prefix filters* in lsm-tree (option B) need a release that offers them, or a fork.
- *Absence by index*: the subject-predicates and object-predicates lists, written already, say which lists a node
  has, so a reader consults them before reading a list that may not exist.

**Recommendation.**

- Absence is proven by index: readers consult a node's predicate lists before its other lists, and new IDs skip list
  reads as today. No existence markers.
- The 128-bit key format keeps the version as a fixed 8-byte suffix after the encoded key and its end code, so a
  prefix filter needs only "all but the last 8 bytes" once lsm-tree offers one.
- Stage 7 measures seeks per read; a fork for prefix filters is taken only if they bind.

**Consequences.**

- The read estimates of this note's discussion assume memtable and cache costs; reads from tables cost a seek per
  level until filters exist.
- `../notes/perf-optimization.md` becomes a read-path note; its options P1–P4 are renamed R1–R4 so they no longer collide with
  this note's.

### P20. What stage 5.5 builds and what it only decides

**Trade-off.**

- P1–P19 mix formats that every later stage stores (IDs, versions, keys, the log record) with runtime choices that
  only matter once replication and the sequencer run.
- Building the cross-partition machinery (sequencer, Calvin commits, change streams) in stage 5.5 lets stages 6 and 7
  run on several partitions, but makes 5.5 most of stage 8.
- Deciding everything in 5.5 and building the formats only keeps 5.5 bounded; stages 6 and 7 then run with one
  partition, a configuration of the same code, not a separate path.

**Recommendation.**

- **Stage 5.5 decides and builds** the formats and the seams:
  - identity: the 128-bit ID (D1 with P7), type prefixes (D2), the bucket (P15, P16);
  - versions (P13) and the 128-bit key format, filter-ready, with split reverse parts (P17, P19);
  - the log record and the log interface with durability tickets (P18, P10's interface), files per partition behind
    it;
  - the metadata group with one replica, holding the catalog, ID blocks and the placement map (D4, P2, P7);
  - the sequencer with one replica, closing epochs and ordering multi-partition transactions (P21);
  - the partition runtime, partition-generic, run with one partition for stages 6 and 7 and with N in `wal_bench`
    (P5 step 1);
  - the crates divided into the three roles, each role its own binary and OCI image under `../../ultragraphdb` (P22).
- **Stage 5.5 decides and stage 8 builds**: the execution of cross-partition commits on the sequencer's order (P3,
  P14), change streams and their aggregation per node, replication and replica placement (P8), node budgets (P4, P11),
  the epoch agent (P12), flush sharing behind the log interface (P10).

**Consequences.**

- Stage 5.5's change carries the specifications of what it builds; the decisions it does not build are recorded in
  this note and specified in stage 8's change.
- The development container's two storage groups of one replica (single-container note) arrive with stage 8.

*Amended by P24 (2026-11-02): stage 5.5 also builds registration, referrer sets, fan-out reverse reads, the epoch
inequality and the refusal of cross-partition deletes; neither stage builds change streams or split reverse parts.*

## Decisions taken before drafting

The user took three more decisions on 2026-11-01, closing the last questions before stage 5.5's specifications.

### P21. The sequencer arrives in stage 5.5

Every commit carries an epoch (P13), so something must advance epochs from the first stage that stamps them. The
sequencer is built in stage 5.5, before anything uses an epoch, as a log-backed state machine with one replica like
the metadata group (D4): it closes epochs at the sequencer's wall-clock time, records each close, and keeps the ordered
log of multi-partition transactions. Partitions stamp their commits with one above the latest close delivered to them
(P24 rule 4). Stage 8
replicates it and executes cross-partition commits in its order (P3, P14); the distribution note's open questions on
epoch length and on placing the sequencer beside the metadata group stay with stage 8's measurements.

### P22. Three binaries, three images

Each role is its own binary under `../../ultragraphdb`: frontend (the endpoint, the metadata group, the sequencer),
compute (query execution) and storage (transactions, the commit step, the log, the store). Each ships as its own OCI
image on the Wolfi glibc base through `glibc_multiarch_img` (D7). A binary carries only its role's code and
dependencies, so each image is the smallest that role allows and a role updates on its own; the roles share the
libraries beneath them. Stage 5.5 builds the three binaries and their images and runs them together for stages 6 and 7.

The development container runs the three binaries as three processes under a supervisor binary, joined by the
transport over Unix-domain sockets; the in-process carrier is dropped (single-container note, approved 2026-11-01).

### P23. The Go golden vectors of the 64-bit formats are retired

The 128-bit ID ends byte-for-byte compatibility with Dgraph's packed-UID format, so stage 5.5 removes the Go golden
vectors of the `uid-lists` spec entirely: `codec_uidpack` and `codec_seek` (`tools/golden/codec.go`,
`ultragraphdb_core_codec/testdata/golden`) and the tests and scenarios reading them. The `key-encoding` spec has no
generated vectors; its scenarios citing Dgraph's bytes are rewritten for the 128-bit layout. The new formats take
vectors generated from their own specifications. The other generators stay; `algo_setops` and the HNSW graph vectors
also carry 64-bit IDs, and stage 5.5 retires whichever of them its formats invalidate.

## The placement decision of 2026-11-02

Two adversarial reviews of the amended P3 and P17, and a TRIZ/ARIZ analysis of what they kept failing on, led to one
more decision. The user adopted it on 2026-11-02. It supersedes P17 and amends P1, P3, P7, P12, P13, P18, P20 and P21
where those sections say so.

### P24. One owner per fact

**Root cause.** Placing data by vertex (P1's subject bucket, adopted to escape the hot-predicate ceiling of predicate
placement) gives every edge, the unit of a graph write, two owners: the subject's partition holds its forward entry and
the object's partition its reverse entry. The amended design compensated with a change stream of derived entries:
receiver logs, seals, two watermarks, versions assigned by the receiver, caps on rollups, a stall policy and sequenced
blind writes. After two reviews it still left nine questions open, and every one of its mechanisms repaired a
consequence of the second owner.

**The inversion.** Every fact stays with its writer. What moves toward the reader is a small, monotone where-to-look
record: the set of partitions that may hold edges to an entity. A record that only grows needs no ordering against the
facts it points at, only the guarantee that it grows before they are written.

**The rules.**

1. **Identity.** A system ID is, from the most significant bit, a 16-bit type prefix, a 64-bit sequence, a 24-bit bucket
   and 24 reserved bits (field order type, sequence, bucket, approved 2026-11-02: the bucket's hashed bits sit below the
   sequence, so a sorted list's deltas stay small). `Bucket::derive` hashes the tenant for the bucket's top 8 bits
   and the declaring type's prefix with the canonical key for its low 16 (P15, P16). Sequences are leased per (type,
   partition) and never reused (P7). An entity's home is the partition the placement map gives its bucket.
2. **One owner.** A partition owns its home entities' state and every entry derived from the edges and values its own
   subjects write: forward lists, subject-predicates lists, its local reverse index (an object to the partition's own
   subjects), object-predicates entries, counts, value indexes and statistics, all written in one local commit, as the
   single-node code of stage 4 writes them. An edge's forward and reverse entries are therefore always consistent. A
   hub's inbound entries are spread over its referrers' partitions by construction, which is what P17's split sought,
   so a reverse key carries no source-bucket part. Nothing is derived in one partition and applied in another: there is
   no change stream, derived keyspace, seal, receiver-assigned version or rollup cap.
3. **Registration is get-or-create.** Registration is per entity and referring bucket group: before a partition first
   commits a reference to an entity `o` it does not own from a subject of bucket group `g`, it sends `o`'s home a
   registration: the IRI's text for a new IRI or the ID of a known one, `g`, and the generation it last saw for `(o,
   g)`. The home makes the registration durable in its log (its durability ticket; the VSR commit in stage 8) before it
   replies with the ID and its last delivered epoch close. The home creates the IRI-to-ID entry if it is absent and adds
   the bucket group to `o`'s referrer set `R(o)`. Registrations are idempotent, batched per home node within each write
   request and issued while the transaction executes; concurrent registrants all receive the same ID and none aborts,
   which replaces the term dictionary's rule that the second creator retries. The referrer keeps a registered mark and
   the generation per `(o, g)`, in a marks keyspace of its own, and copies `o`'s dictionary entries in both directions
   into its own dictionary at its first reference to `o`; P1's volatile IRI-to-ID cache stays. The dictionary is naming
   state only: existence and type membership are read from committed data, never from a dictionary entry. A predicate or
   graph IRI another partition owns is got or created at its home the same way before first use, joining no referrer
   set, which concerns IRIs and blank nodes in the object position only.
4. **The epoch inequality.** A registration's epoch is at most the commit epoch of every write referencing it, on every
   write path; this is the lemma the rest rests on. A partition stamps its commits with exactly its last delivered close
   plus one and never advances on its own: when its 22-bit sequence would run out, it asks the sequencer to close now
   (`epoch_ms_min` still honoured) and waits, refusing with `Busy` once the request has gone unanswered for the setting
   `close_request_ms`. A snapshot read waiting on the open epoch makes the frontend ask its sequencer for an early
   close. A registration's reply carries the home's last delivered close `c`, and the registrant commits only after
   close `c` has been delivered to it. A sequenced submission (stage 8) carries the largest reply epoch `c + 1` among
   its registrations, and the sequencer places it at or above that epoch. This revises P13's local advance: a partition
   stamps exactly one above its last delivered close, so it never stamps an epoch whose close has been delivered to it,
   and it holds the catalog versions announced with that close (rule 11) when it stamps the epoch they take effect at.
5. **Referrer set.** `R(o)`, kept at `o`'s home, is a monotone superset of the subject bucket groups holding edges to
   `o`. A bucket group is 64 contiguous buckets, 2^10 groups per tenant run. Each member carries the epoch it was
   added at and a generation, under MVCC. The home adds its own subjects' groups in the commit that first references
   `o` from them. A registration naming the member's current generation, or naming any generation for a group not in
   `R(o)`, adds or renews the member at one generation above the one named; a registration naming an older generation
   is a repeat and is answered with the current one. Groups resolve to partitions through the placement map
   at a read's epoch, so moves, splits and restores rewrite nothing.
6. **Reads.** A forward read routes by bucket. A reverse read reads `R(o)` at epoch `e` at the home once the home has
   closed `e`, then fans out, batched per node, to the member partitions, each answering from its local reverse index at
   `e` once it has closed `e`; a large frontier uses a merge scan. A bound literal object, which has no home or referrer
   set, index lookups and lookups by reverse count fan out the same way over the tenant's partitions, a reverse count
   being the sum of the partitions' counts. A snapshot read is complete or it waits, never partial; a read reaching its
   deadline fails with an error naming the lagging partition; in stage 8 followers serve reads. An opt-in inbound
   summary cache at the home stays a later option, taken only if measurement asks for it; correctness never depends on
   it.
7. **Transactions.** A read-write transaction binds to the partition owning the first subject it reads or writes, or,
   when its first operation names no subject, to the partition owning the tenant's whole run if one does, and reads
   and writes only what that partition owns; registrations are idempotent pre-steps outside it. It makes a reverse or
   fan-out read only when its partition owns the tenant's whole run, always so with one partition. Everything else is
   cross-partition: a forward read of another partition's subject, a fan-out lookup or scan or a reverse read over a
   tenant spread over several partitions, cascading the delete of inbound edges, multi-subject writes across
   partitions, blind writes to other partitions (P3's blind multi-partition path, which becomes sequenced with no
   validation exchange, at one epoch of latency), and a Drop whose tenant spans several partitions. Stage 5.5 refuses
   it with `CrossPartition`; stage 8 sequences it. A `@unique` value, written by any write, plain writes included, stays
   a sequenced claim at the owner of the value's bucket, computed from the predicate's ID and the value's encoding (P3);
   with one partition it is local. A read-only transaction reads across partitions at its snapshot.
8. **Deletes.** Stage 5.5's changesets delete by pattern, and a delete by pattern that reaches another partition's
   state, a subject or inbound entries another partition owns, is cross-partition (rule 7) and refused. The entity
   delete UGS brings in stage 6 writes a tombstone at the entity's home and, executed across partitions in stage 8, a
   sequenced share at every member of `R(o)` that fences `o` locally; a later commit referencing `o` aborts with
   `EntityDeleted`, a registration after the tombstone is answered "deleted", and re-creating the IRI allocates a new
   ID.
9. **Reclamation** (specified now, built in stage 8). A partition reports "no edges to `o` as of epoch `e`" with its
   generation, in the same commit that clears its mark; the home removes the member only if the generation is
   unchanged, as an MVCC write, and physical deletion waits for the history-retention horizon. The home's IRI-to-ID
   entry is reclaimed once `R(o)` is empty below the horizon and `o` has no committed data. IDs are never reused.
10. **Durability and restore.** A cluster restores at one closed epoch, since referrer sets and marks span partitions;
    a single partition restores only from its replicas; a restored partition abandons its open sequence blocks, whose
    sequences it may have issued after the restore point; a tenant drop is sequenced at an epoch, and tenant
    numbers are never reused.
11. **Catalog activation.** The metadata group commits a catalog version as pending; at its next close `c + 1` the
    sequencer assigns every pending version the effective epoch `E = c + 2` in its own durable close record, which
    announces it, and the group learns `E` from that close, so every partition holds the version before it can stamp `E`
    (rule 4), and a frontend crash before the close leaves the version pending for the next one. This corrects a defect
    the protocol review found: "at least one above the latest closed epoch" let a partition stamp the version's epoch
    before the version reached it.

**Alternatives considered.**

- *The derived-stream design*, the amended P3 and P17: forward entries with the subject, reverse entries and the
  object's indexes shipped by change stream to the object's partition. Rejected: each of its mechanisms exists to keep
  a second owner of one fact in step, a snapshot read of reverse entries waits on the slowest stream feeding it, and
  nine questions stayed open after two reviews.
- *Partition-local handles with a derived entity key*: each partition names entities by handles of its own and derives
  a global key, the key's hash, when facts cross. Rejected: an entity has one name per partition holding it, ID lists
  from different partitions no longer merge by ID, a bucket move translates every handle it carries, and the derived
  key brings back the collision check of a hash ID.
- *Hash-derived IDs*, the whole ID a hash of type and key, so any partition computes it without asking the home.
  Rejected: collisions must still be detected at the home, so the dictionary check stays; IDs lose creation order, so
  packed lists' deltas grow to the hash's width; a blank node has no key; and reverse reads still need `R(o)` at the
  home, so the round trip a registration costs is spent anyway.
- *Global value indexes*, partitioned by the value's hash, so a lookup asks one partition. Rejected: a common value,
  and every value of a low-cardinality predicate, puts its whole list on one partition, which recreates the
  hot-predicate ceiling P1 escaped, and every indexed write becomes cross-partition. Value indexes stay with their
  subjects and lookups fan out; a secondary `@unique` value alone stays a claim at its hash's owner, since uniqueness
  cannot be checked locally.

**Accepted costs.**

- An inbound point read touches `min(d, partitions)` partitions for an in-degree `d` and needs one extra round, `R(o)`
  at the home first: about 20× the store probes of one reverse list at in-degree 20. Measured on stage 5.5 (*The
  inbound point read beside P24's estimate*): `1 + 2k` probes for `k` partitions asked against one partition's 2, 9.5×
  at in-degree 20 on 12 partitions, which asked 9, and 2.7× the time in one process.
- A partition's first reference to an entity it does not own waits for one durable round at the entity's home. Later
  references find the mark and wait for nothing; a hub's home answers at most 2^10 registrations per tenant run for
  it over its life, one per bucket group, whatever the ingest.
- Index lookups, reads of a bound literal object and lookups by reverse count ask every partition of the tenant; an
  exact reverse count is the sum of the referrers' partition counts.
- `R(o)` only grows until stage 8 builds reclamation; it is bounded by the 2^10 groups of a run.

**What it amends.**

- *P1, consequences:* a partition's first reference to an entity it does not own registers at the home; the IRI-to-ID
  cache stays; the dictionary is naming state, and existence is read from committed data.
- *P3:* an edge's reverse entry and the object's indexes stay in the subject's partition, with no change stream; the
  blind multi-partition path becomes sequenced, with no validation exchange, at one epoch of latency; snapshot reads of
  reverse entries wait for closed epochs, not for a stream's watermark.
- *P7:* the field order is type, sequence, bucket, reserved.
- *P12:* there are no change streams between partitions; the epoch agent and the batching of sequenced work stay.
- *P18:* the log record carries each transaction's registrations as a referrer, entries copied in both directions,
  drops as operations, deletes by pattern as their removals, session-table entries, index-build records and the
  registrations a home accepts, each with its own commit version; a round holds one epoch.
- *P13:* a partition stamps exactly its last delivered close plus one and asks the sequencer for a close when its
  sequence would run out, in place of advancing on its own.
- *P17:* superseded. No reverse list lives with its object, a hub's inbound entries are spread over its referrers'
  partitions, reverse keys carry no source-bucket part, and the placement map records no split nodes. `rdf:type` is
  still answered from type-prefix ranges.
- *P20:* stage 5.5 builds registration, referrer sets, the local reverse index, fan-out reverse reads, the epoch
  inequality, the refusal of cross-partition deletes by pattern and a Lean model of rules 4, 5, 6, 9 and 11, with
  catalog announcements and tombstones; stage 6 brings
  the entity delete and its tombstone; stage 8 builds the sequenced cross-partition work, the delete shares and
  reclamation. Neither builds change streams or split reverse parts.
- *P21:* a partition stamps one above the latest close delivered to it (rule 4).
- The distribution note's lines on reverse entries following by change stream in the KV stage give way to this
  section; its change streams for reverse tiles stay with the tile stage.

**Consequences.**

- The `partitions`, `term-dictionary`, `quad-reads`, `quad-storage`, `key-encoding`, `transactions`, `sequencer`,
  `commit-rounds`, `metadata-group`, `schema-changes`, `wire-protocol` and `system-identity` specifications of stage
  5.5 state these rules, with `changesets`, `mapping-contract`, `index-maintenance`, `schema-storage`, `uid-lists` and
  `write-ahead-log`, and a Lean model of rules 4, 5, 6, 9 and 11 is proved before the partition code is written.
- `wal_bench`'s N partitions (P5 step 1) exercise registration: a share of each partition's edges points at entities
  other partitions own.
- The protocol review's resolutions, each following from system IDs never leaving the database: a statement without a
  statement ID is named to clients by its quad, unique among statements without IDs; a statement with one, because it
  carries properties or repeats a quad as a parallel edge, is named by its reifier (the `mapping-contract` spec's
  presentation of a statement ID), a skolem IRI minted as a blank node's, `…/.well-known/genid/{bucket}-{random}` with
  the statement's bucket, and held in the dictionary as the reifier's name. A client addresses a statement's properties,
  or one of several parallel edges, through that IRI, as RDF 1.2 reification does. A Commit result names the node an
  operation found or created by its IRI or skolem IRI; a transaction names on Begin the level, session token and
  deadline its reads share; a resume key carries its last row's terms as text, the pattern's hash and a checksum.

## What stage 5.5 built

Stage 5.5's change (`../changes/archive/2026-11-09-stage-5-5-three-role-refactor`) built every decision P20 and P24 assign it, each
group on one replica; P20's list of what stage 8 builds is backlog item 11. Each row names the decision, what runs and
where.

| Decision | Built | Where |
|---|---|---|
| D1, P1, P7, P15, P16, P24 rule 1 | The 128-bit system ID, type prefix, sequence, bucket and 24 reserved bits from the most significant bit; the bucket's top 8 bits hashed from the tenant, its low 16 from the declaring type's prefix and the canonical key; sequences leased in blocks per (type, partition) and never reused | `ultragraphdb_core_ids` (`src/types/system_id/mod.rs`, `src/types/bucket/mod.rs`, `src/types/tenant.rs`); blocks through `ultragraphdb_traits_blocks`' `BlockSource`, leased by the metadata group |
| P2, D4 | Buckets assigned to partitions as ranges, tenant runs, bucket groups resolved at an epoch, preferred leaders, each layout from the epoch it applies at | `ultragraphdb_cluster_placement` (`src/types/placement_map/mod.rs`) |
| D4 | The metadata group with one replica: tenants, catalog versions, sequence blocks, the placement map, principals and the registry of sessions, a state machine over the ordered log | `ultragraphdb_cluster_metadata` (`src/types/metadata_group/mod.rs`), run by the frontend's owner in `ultragraphdb_common_frontend` |
| P21, P24 rule 11 | The sequencer with one replica: epochs closed at its wall-clock time or early on request, each close recording the catalog versions taking effect at `c + 2` and the placement version, `as of`, the ordered log of multi-partition transactions | `ultragraphdb_cluster_sequencer` (`src/types/sequencer/mod.rs`); the metadata group learns each close in `src/types/metadata_group/metadata_group_catalog.rs` |
| P13, P24 rule 4 | A version of a 42-bit epoch and a 22-bit sequence; a partition stamps exactly one above the latest close delivered to it, asks for a close when its epoch is full and refuses its commits as busy after `close_request_ms` | `ultragraphdb_core_ids` (`src/types/version/mod.rs`); `ultragraphdb_transactions_commit` (`src/types/epoch_stamp/mod.rs`, `src/types/close_request.rs`) |
| P19, P24 rule 2 | Keys over 128-bit IDs, a reverse key ending with its object, the marks and referrers keyspaces; a partition's forward, reverse, index and statistics entries written in one local commit | `ultragraphdb_core_keys` (`src/types/keyspace.rs`, `QuadKey`); `ultragraphdb_store_apply`, `ultragraphdb_store_quads` |
| P9, P18 and P24's amendment | The logical log record: resolved quads with their system IDs, the round's dictionary entries, registrations, operations, session-table entries, one epoch, catalog and derivation version per round; the log the only synced stream, fjall's journal persisted manually and never synced | `ultragraphdb_protocol_wire` (`src/types/write_set_record/mod.rs`); `ultragraphdb_transactions_log`'s `Wal`; `ultragraphdb_store_kv` (`src/types/fjall_store/mod.rs`) |
| P10's interface | The ordered-log interface with durability tickets, one log per partition behind it | `ultragraphdb_traits_log` (`src/traits/ordered_log.rs`, `src/types/durability_ticket.rs`) |
| P24 rules 3 and 5 | Registration as get-or-create at the entity's home, durable before its reply; the referrer's marks; referrer sets whose members carry their epoch and generation | `ultragraphdb_cluster_partition` (`src/types/home/mod.rs`, `src/types/reference.rs`); `ultragraphdb_store_quads` (`src/types/referrer_set/mod.rs`) |
| P24 rule 6 | Reverse reads and summed reverse counts through referrer sets at a closed epoch, fanned out to the member partitions; compute's routing, fan-out and merge in key order | `ultragraphdb_cluster_partition` (`src/types/fanout/mod.rs`, `src/types/reverse_query.rs`); `ultragraphdb_common_compute` |
| P24 rules 7 and 8 | A read-write transaction bound to the partition owning its first subject; cross-partition work, a delete by pattern reaching another partition's state included, refused with `CrossPartition` | `ultragraphdb_transactions_txn` (`Transaction::bind`, `src/types/transaction/mod.rs`); `ultragraphdb_cluster_partition`'s `PartitionError` |
| P24, rules 4, 5, 6, 9 and 11 | The placement model in Lean, built with no `sorry`; its invariants checked after every step of partitions on the simulator under every fault class | `../../lean/PlacementModel`; `ultragraphdb_tool_test` (`src/types/partition_workload/mod.rs`) |
| P20, P5 step 1 | The partition runtime, partition-generic: one partition by default (`partition_count` in the `dev` profile), N in one process in `wal_bench --partitions`, each on a pinned compio runtime with its own store, log and seeded generator and a share of edges to other partitions' entities | `ultragraphdb_cluster_partition` (`PartitionHost`), hosted by `ultragraphdb_common_storage`; `ultragraphdb_tool_bench` (`src/utils/partitions_run.rs`) |
| P22 | The frontend, compute and storage binaries and the supervisor, joined over Unix-domain sockets in the development image; each role's image on the Wolfi glibc base and the seccomp profile allowing io_uring | `../../ultragraphdb/ultragraphdb_frontend`, `ultragraphdb_compute`, `ultragraphdb_storage`, `ultragraphdb_supervisor`; `glibc_multiarch_img` in each role's `BUILD.bazel`; `bazel/oci_images/seccomp/ultragraphdb.json` |
| P23 | The packed list and the posting list with 128-bit IDs under their own version bytes, their golden values generated from their specifications | `ultragraphdb_core_codec` (`testdata/golden/formats.py`) |
