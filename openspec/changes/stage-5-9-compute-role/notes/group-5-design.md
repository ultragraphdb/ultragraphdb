# Group 5: the endpoint executes reads — design (tasks 5.3 to 5.6)

Base commit `18e41fa9` (task 5.2). This note fixes the seam, the frontend binary's wiring, how a read is built without
ComputeRead, where a read yields, the worked examples, the scenarios and the open questions, before phase 2. Section and
law numbers are `PROTOCOL.md`'s (`P §`) and `notes/laws.md`'s; cases are `notes/corner-cases.md`'s.

## 1. The seam

One executor serves both callers (design D2): `ComputeSession`, the compute binary's role session, until task 6.4, and
`ClientSession`, in process. Both drive one public type, `ReadExecutor`, which holds the reads of one connection. The
session hands it a read as a value and takes its frames back as values (D3); credit and Cancel go forward as calls.

| Direction | Message | Type, function | Reuses |
|---|---|---|---|
| session → executor | a read handed over | `ReadExecutor::start(&mut self, stream: u64, read: ReadRequest)` | `ReadRequest` (made `pub`), `OpenRead`, `ResultSink`, `run_read` |
| session → executor | room for the next Result frame: the frame before left and the stream holds credit | `ReadExecutor::ready(&mut self, stream: u64)` | `OpenRead::room`, the `()` per frame `ComputeSession` sends today |
| session → executor | Cancel, or the read answered otherwise (a Commit's, an Abort's, a code 23's end) | `ReadExecutor::cancel(&mut self, stream: u64)`: the `OpenRead` dropped, its task cancelled | `OpenRead::_task` |
| session → executor | the connection's end | `ReadExecutor` dropped with its session: every task cancelled | |
| executor → session | a Result frame, the last flagged `LAST`; a refusal as an Error frame flagged `LAST` | `async fn ReadExecutor::next(&mut self) -> (u64, Vec<u8>)`, the stream the session started it on and the frame | `Pushed::Frame`, unchanged |
| executor → executor | a read's task ended | consumed inside `next` | `Pushed::Ended`, unchanged |

`ReadExecutor` (`src/types/read_executor/mod.rs`):

```rust
pub struct ReadExecutor { core: Rc<ComputeCore>, open: Option<OpenRequests>, reads: HashMap<u64, (u64, OpenRead)>,
                          streams: HashMap<u64, u64>, next: u64, pushes: UnboundedSender<Pushed>, pushed: UnboundedReceiver<Pushed> }
impl ReadExecutor {
    pub fn new(core: Rc<ComputeCore>) -> Self;
    pub fn with_open(self, open: OpenRequests) -> Self;   // ComputeSession's count; ClientSession counts its reads itself
    pub fn start(&mut self, stream: u64, read: ReadRequest);
    pub fn ready(&mut self, stream: u64);
    pub fn cancel(&mut self, stream: u64);
    pub async fn next(&mut self) -> (u64, Vec<u8>);
}
```

Decisions:

- **The executor numbers what it starts.** `start` gives each read the executor's next number, sets `ReadRequest::stream`
  to it, and builds the `ResultSink` and `run_read`'s Error header on it, as a role link numbers its streams. `next` maps
  the number back to the session's stream, stamps the frame's header with it (`Header::stamp_stream`), and drops a message
  of a number it no longer holds as late. Without the number, a read started again on its stream (a first read moved by
  a stale map, `after_refresh`) could lose its task to the `Ended` of the read it replaced, since `ClientSession` takes
  `events` and the executor's frames from two channels.
- **A refusal stays a frame.** `run_read` builds the Error frame as today; `ClientSession` reads it with
  `ErrorReplyView` into the `ErrorReply` its rules take (`read_refused`: code 23, Error 1 of `detail` 1), as `RoleLink`
  read compute's Error before. `Pushed` keeps its two variants, `ComputeSession` needs no new arm, and both callers get
  the same bytes (L5.14).
- **Credit is the connection's.** The client connection's `Outbound` already meters Result frames by the Welcome's
  `initial_credit` and each Credit, and calls `Session::ready` once a frame left with credit left. `ClientSession`
  implements `ready` by `ReadExecutor::ready`, exactly as `ComputeSession` does; the read builds its next batch only on
  room (`ResultSink::send_batch`), so no batch ever waits on credit in `Outbound`, and the client's `max_frame` and credit
  apply directly (D4). `ClientRead::credit` stays only for code 4's message (`past_limit`).
- **`ComputeSession` becomes a thin adapter** over `ReadExecutor::with_open(open)` in phase 4: `answer` → `ReadRequest::of` →
  `start`; `next_push` → `next`; `cancelled` → `cancel`; `ready` → `ready`. One copy of the open-read bookkeeping.

`ClientSession` in phase 4: a field `reads_run: ReadExecutor` built from `EndpointCore`'s `ComputeCore`; `next_push`
selects between `events` and `reads_run.next()`; a frame from the executor goes to `relay_result` (Result) or, read by
`ErrorReplyView`, to `read_refused` (Error); `dispatch_read` builds the `ReadRequest` (section 3) and calls `start`;
`read_cancelled`, `read_cancelled_answering`, `cross_ended` and `end_reads` call `cancel` where they called
`RoleLink::cancel`; `read_credited` relays nothing. The relay's paths (`next_compute`, `tried`, `members_again`,
`read_closed`, `hung_at`) are reached by no read and go in task 5.7.

## 2. The frontend binary

| | Today | After this group |
|---|---|---|
| Where a core's `ComputeCore` comes from | none | `frontend_role` (core 0) and `serve_core` (cores 1 to n) build one per endpoint core: `ComputeCore::new(cluster.clone(), system_address.clone(), read_times_of(config.storage().scan_stall_ms(), config.storage().read_retain_ms()), "frontend")`, handed to `Clients::core` and set by `EndpointCore::with_compute` |
| Placement | `SystemLink`, subscribed, into `RoleLinks` | the same for writes; the executor's `ComputeCore::placement` asks Placement once over its own connection to the group and holds it for the process (fixed in this stage) |
| Partitions' addresses | `SystemLink`'s Members into `RoleLinks` | the same for writes; `ComputeCore::address_of` asks Members on its first dial of a partition and again after a dial fails |
| What it dials | one `RoleLink` per partition (Bind streams, one-frame writes) and one per compute process (ComputeRead) | one `RoleLink` per partition and one `Link` per partition, both as the SCRAM user `frontend`; no compute process |
| Join | none | none: only the compute binary calls `join_group` and `hold_peers`; the system group serves Members, Placement and Catalog to every role (`common_system`'s `served`) and storage serves Scan and Closed to any authenticated role |

Two connections per core and partition stand until group 4 puts the reads on `RoleLink` (D2) and task 4.3 removes `Link`
and `ComputeCore`'s own asks. Readiness is unchanged: it never waited for a compute process. The frontend's settings file
carries `[storage] scan_stall_ms` and `read_retain_ms` where a harness sets them for compute today; no profile sets either.

## 3. Building `ReadRequest` without ComputeRead

`ReadRequest` keeps its fields. It is split by when they are known:

- **`ReadPattern`** (`src/types/read_pattern.rs`, `pub`), what the request itself asks, built once when the read is taken
  and held in `ClientRead` in place of `body`: the graph, subject and predicate IRIs, the object term, `limit`, `resume`
  and the pattern's hash. `ReadPattern::of_read(read: &ReadView<'_>) -> Result<ReadPattern, ErrorReply>`, the same slot
  rules `ReadRequest::of` applies today.
- **`ReadRequest::new(pattern: ReadPattern, tenant: u64, read_only: bool, level: ReadLevel) -> ReadRequest`** at
  dispatch, then the setters `with_partition(u32)`, `with_transaction([u8; 16])`, `with_read_version(u64)`,
  `with_named_version(u64)`, `with_epoch(u64)`, `with_principal(u64)`, `with_deadline_ms(u32)`, `with_max_frame(u32)`,
  each one value, so no two of one type can swap. `ClientSession::compute_read`'s chain moves over as it stands, minus
  `limit`, `credit` and the term table; `ReadRequest::of(&ComputeReadView, stream)` stays `pub(crate)` for
  `ComputeSession` until task 6.4.

By request:

- **Read.** `read()` takes `ReadView::from_frame`, then `ReadPattern::of_read(&view)`.
- **Execute.** `execute()` keeps its order of refusals (§11.7) and `StatementCache::execute`, whose `(TermTable,
  QuadPattern)` is the table §11.8 states: the statement's as its Prepare sent it, then each parameter's terms from the
  Execute's table, a triple term's parts first, a term held already keeping its index (`TermTable::import`). It builds
  `Read::new(transaction, limit, terms, pattern, resume)` in place of a ComputeRead, validates the frame within
  `MAX_FRAME_SIZE` (code 6 past it, as today), and takes the same path as a Read. `Read` is a client command and outlives
  ComputeRead, which task 1.5 removes.
- **Query.** Refused at the endpoint with code 7, "a Query is served from stage 7" (`ClientSession::query`); it builds no
  read.

**The pattern hash.** `ReadView::pattern_hash(&self) -> u64` in `ultragraphdb_protocol_wire`, beside `ResumeKey`: XXH3-64,
seed 0, of the body's sections 0 to 2 read in place, the rule `ComputeReadView::pattern_hash` holds today and that task
1.5 removes with its view. The bytes are equal:

1. `ComputeReadView::from_frame` hashes `body[offset(0) .. offset(2) + len(2)]`. With `F = 0` and four sections (§4.1),
   section 0 starts at `align8(32) = 32` and is 32 bytes (§4.5); section 1 starts at `align8(64) = 64` and is `24n`
   bytes for `n` terms (§4.4); section 2 starts at `align8(64 + 24n) = 64 + 24n`, since `24n` is a multiple of 8. The
   slice is pattern ‖ records ‖ text with no padding byte: §11.8's three runs.
2. A Read's body has the same layout: sections 0 to 2 the pattern, the records and the text, section 3 the resume key
   (`Read`'s builder writes `[pattern.write(), records, text, resume]`, as `ComputeRead`'s does). The same rule over the
   same sections gives the same bytes.
3. Before this group the frontend sent the client's Read body unchanged as the ComputeRead's (`dispatch_read`:
   `encode_frame(&state.body)`, `state.body = view.body().to_vec()`): compute hashed the client's own sections 0 to 2,
   which `ReadView::pattern_hash` now hashes in place.
4. For an Execute the frontend built `ComputeRead::new(tenant, …, terms, pattern, resume)` from
   `StatementCache::execute`'s pair; now it builds `Read::new(transaction, limit, terms, pattern, resume)` from the same
   pair. Both bodies write `pattern.write()`, `terms.sections()` and `resume` as sections 0 to 3: the same bytes.
5. `scope` is unchanged: `ReadRequest::scope` is `xxh3_64(transaction)` for a read-write transaction, set by
   `with_transaction(txn.identity)` as `ComputeRead::with_transaction` was, and the read version otherwise, set by the
   same `with_read_version`/`with_named_version` calls with the same values (0 at levels 1 and 2).

So `pattern` and `scope` of a key written before this group match after it. Whether such a key still *decodes* turns on
open question 2: §11.8's layout is not the one `ResumeKey` writes.

## 4. The step bound (D3)

A step is what a read's task does between two yields. It yields:

1. **After each Entries frame it consumes**: at the end of `ScanStream::next`, once the frame was read into the caller's
   page or value, which every Scan of the executor passes through (the merge's `fill`, `finish_gate`, `values`,
   `name_ahead`, `read_names`, the lookups of `compute_core_lookups.rs`). The executor sends no Frontier in this stage,
   so no Neighbours frame arrives; a Frontier's stream takes the same point when it does.
2. **After each Result frame it builds** but the last: `ResultSink::send_batch` pushes the frame and waits for its room.
   The wait cannot be ready at once, since the room of a frame is given only once that frame left, which needs the
   connection's task to run (`flush`, `take_ready`).

A yield goes to the back of the core's run queue (`yield_once`, as `Link`'s reader does). `ComputeCore::steps()` counts
the steps the core's reads took, the counter shared with each `Link` and `ResultSink` of the core, for the step laws.

Two places consume several frames in one poll today and change: `merge`'s `try_join_all` over every partition's first
`fill`, which goes with partition order (open question 1: the merge drains one partition after another, so no first fill
waits beside another), and `first_at`'s `try_join_all` over Closed, whose answers are no Entries frames and stay.

What a step holds: the parse of one Entries frame of at most the role connection's `max_frame` into a page, or the
copying of rows into one Result batch of at most the client's `max_frame`; never both in full, since a page's rows go
into batches across the steps that build them. With the `Page::add_row` fix (task 5.4) a page holds each object
encoding once, so a page's bytes are bounded by its frame's.

**A commit waits at most one step: L5.9 and L5.10.** compio's executor runs its ready tasks first in, first out
(`compio-executor`'s hot queue). A read that yields is queued behind every task already ready, so between two of its
steps every other ready task runs once (L5.10). A connection's task holding a Commit is ready from the moment the driver
reports its bytes; it then runs before the read's next step and hands the Commit to its partition's `RoleLink`, whose
writer task is queued behind the read: the Commit is relayed within one step of its task being ready and written within
two. The driver is polled when the run queue empties or after `event_interval` task polls, 61 by default
(`RuntimeBuilder::event_interval`): from the bytes' arrival a read alone on its core may take up to 61 steps before the
Commit's task is ready (open question 3). L5.9 is stated over the scheduler's interleavings, as written; task 8.3
measures from arrival.

## 5. Worked examples

Common path: the client's frame is taken by `serve_connection`'s `take` → `ServerConnection::receive` → `Inbound::Request`
→ `ClientSession::answer` → `request`. The executor's frames return by `ClientSession::next_push` → `ReadExecutor::next`
→ `ClientSession::read_frame` (new) → `relay_result` or `read_refused` → `out` → `Push::Frame` → `Outbound::push` →
`flush`. "Step" marks a yield of the read's task.

**(a) One subject, one partition, one batch.** `read` → `ReadView::from_frame` → `ReadPattern::of_read` → `take_read`
(`check_table`, `check_pattern`, `binds_nobody`) → `ClientRead` queued → `pump` → `send_next_read` → `dispatch_read` →
`ReadRequest::new(…).with_read_version(v).with_epoch(e).with_principal(p).with_deadline_ms(left).with_max_frame(mf)` →
`ReadExecutor::start(5, read)`: number `k`, `ResultSink::new((cluster, k), mf, columns, pushes, rooms)`,
`spawn(run_read(core, read, sink, pushes))`. The task: `answer` → `resume_terms` (none) → `ComputeCore::placement` →
`ReadRoute::of` → `Partitions([p])` → `opened_parts` → `gated_parts` → `ComputeCore::link(p)` → `open_gate` →
`finish_gate` → `ScanStream::next` (step) → `plan` → `merge` → `fill` → `next_frame` → `ScanStream::next` (step) →
`name_ahead` → `ScanStream::next` (step) → `cells` → `ResultSink::push_row` per row → `finish(vec![])` →
`send_batch(true, …)` → `Pushed::Frame(k, frame)`; `run_read` → `Pushed::Ended(k)`. The session: `next` → `(5, frame)`
stamped → `relay_result` → floor and `first_read` updated → `LAST` → `read_answered` → `pump`. `Outbound` sends it and
closes stream 5. `next` later takes `Ended(k)` and forgets `k`.

**(b) Unbound subject, three partitions, six batches, `initial_credit` 4, Credit 1 twice.** `ReadRoute::of` →
`Partitions([1, 2, 3])` → `gated_parts` sends three dictionary Scans, then takes each answer (a step each). `merge`
drains partition 1, then 2, then 3, each in its entries' order (§9.1, open question 1). Every full batch:
`ResultSink::send_batch(false, …)` → `Pushed::Frame` → room wait (step). Frames 1 to 3 leave with credit left:
`flush` → `take_ready` → `ClientSession::ready(5)` → `ReadExecutor::ready(5)` → room → next batch. Frame 4 leaves with
credit 0: `take_left`, no room; the task waits. Partitions 2 and 3 hold their Scans' first frame meanwhile; one silent
past `scan_stall_ms` ends its Scan, and `go_on_after_stall` reads on from its last key at the same version when its turn
comes. The client's `Credit(1)` → `take` → `receive` (`Outbound::grant`) → `Inbound::Handled` →
`ClientSession::credited` → `read_credited` (the mirror) → `flush` → `take_ready` → `ready(5)` → room → frame 5 (step),
which leaves with credit 0. The second `Credit(1)` → room → `finish` → frame 6 flagged `LAST`; no room wait; the task ends.
C5.29.

**(c) An Execute resumed from a key `K`.** `execute` → refusals of §11.7 → `StatementCache::execute` → `(terms,
pattern)` → `Read::new(transaction, limit, terms, pattern, K)` framed and validated within `MAX_FRAME_SIZE` →
`ReadPattern::of_read` (its hash the one `K` holds, section 3) → `take_read` → as (a) to `start`. The task:
`resume_terms` → `ResumeKeyView::read(K)` (§11.8's checks in order) → `pattern` and `scope` equal the read's →
`ResumeTerms::of` → `opened_parts` → `resume_at` → `ids_at_homes` (one Scan of one-key ranges per home, a step each) →
`PartitionRows::resume_at`: with §11.8's `partition` `q`, partitions below `q` are passed over, `q` reads on strictly
after the row and every later one from its start → `merge` as (b), ending with a new key when `limit` stops it.

**(d) A Cancel between two Result frames.** The client's Cancel → `take` → `receive`: `Outbound::cancel(5)` drops what
waits on stream 5 → `Inbound::Cancelled` → `ClientSession::cancelled(5)` → `read_cancelled` → `ReadExecutor::cancel(5)`:
the `OpenRead` dropped, its `JoinHandle` cancelling the task; compio never polls a cancelled task again and drops its
future at the task's next slot, so each `ScanStream`'s `Drop` queues its Cancel on its `Link` and the pins go. A frame
the task pushed before is late in `next` and dropped. `read_answered` → `pump`: a Write held behind the read leaves; the
transaction goes on. L5.8, C5.38, C5.39, C5.43.

**(e) The client's connection closes mid-read.** `serve_connection`'s `run` ends → the `ClientSession` drops → its `Drop`
cancels the transactions' Bind streams as today → its `ReadExecutor` drops → every `OpenRead`'s task cancelled, as (d).

**(f) A Commit on the same core while (b) runs.** Connection B's Commit is reported by the driver → B's task is queued
behind (b)'s task, which yielded after its last step → B's `take` → `ClientSession::commit` (B) → the Commit handed to
the partition's `RoleLink` → (b)'s next step → `RoleLink`'s writer task writes the Commit. Relayed within one step of
B's task being ready, written within two (section 4, L5.9).

## 6. Scenarios

Owning bdd targets: `common_compute`'s, over `ClientSession` with played partitions (`endpoint_stage`, `played_stage`);
`ultragraphdb_tests`', whole clusters of processes; `ultragraphdb_frontend_tests`', the frontend binary.

| Requirement, scenario (task 5.3) | Gherkin today | Binds it |
|---|---|---|
| quad-reads, "A read's step is bounded by one frame": "A commit is relayed between a read's frames", "A read's step stays within one frame" | none | `common_compute`, a new `features/quad-reads/step.feature` in `bdd_quad_reads`, read through `ComputeCore::steps` |
| quad-reads, "Compute runs each read on one core": "A fan-out stays on its read's core" | `compute_tests` (ComputeRead) | `common_compute`, `features/quad-reads/cores.feature` |
| same: "A transaction's reads share its writes' core" | none | `common_compute`, same file; group 4 binds it again once both leave on one connection |
| quad-reads, "Compute times a partition that stops answering": both scenarios | none (`compute_tests` holds a ComputeRead form) | `common_compute`, `features/quad-reads/hung.feature` |
| quad-reads, "A read's batches fit the client's frame": "A batch stops at the client's frame", "A row too large…", "A resume key that does not fit goes alone" | `compute_tests` (ComputeRead) | `common_compute`, `features/quad-reads/batches.feature` |
| same: "A read's stream starts with the client's credit", "A Cancel stops a read in the turn it is read" | none (`compute_tests`: "…the frontend's credit") | `common_compute`, `batches.feature` |
| quad-reads, "A read through compute is a read of its transaction" | 11 in `common_compute`'s `order.feature` or `ultragraphdb_tests`; "A scan through compute holds no list…", "A read past the read set's room…", "A span recorded after a Cancel…" in `compute_tests` alone | the 3 in `ultragraphdb_tests`' `quad-reads/transaction_reads.feature`; `order.feature`'s steps naming ComputeRead or "sends the read to compute" reworded to the delta's text |
| quad-reads, "A read's transaction names its level" | all in `ultragraphdb_tests`' `levels.feature`/`spread.feature` or `order.feature`, but "A monotonic-fast read does not wait for a lagging partition" (`compute_tests` alone) | that one in `common_compute`, played partitions' Closed scripted |
| client-endpoint, "Requests are routed to the partitions that serve them" | all 10 in `common_compute`'s `routing.feature` | unchanged target; three When-steps ("compute answers code 23") played by a partition's Scan refusal, one Then-step reworded; the feature's title renamed |
| client-endpoint, "Errors reach the client through the error table" | all 3 in `errors.feature` | unchanged |
| client-endpoint, "Compute finds the partitions in Members": "A storage process restarted at its address is dialled again" | `frontend_tests` | unchanged; its read now redials through `ComputeCore::link` too |
| same: "Compute listens at no role address", "Members names no compute node" | the first as "The frontend listens at no role address" | not in group 5 (open question 6) |
| client-endpoint, "Compute bounds what a connection's reads hold back" | `held.feature`, `processes.feature`, `shared.feature` | unchanged |

Removed by task 5.7, with their steps and the compute role `RoleDouble` plays: `common_compute`'s
`features/client-endpoint/compute.feature` (9 scenarios, target `bdd_compute`); `system_link.feature`'s "A compute
process that joins later is found before a read is refused"; `order.feature`'s "An Execute too large for compute's frame
is refused" (or kept reworded for the Read frame's bound, open question 8); `ultragraphdb_tests`' `processes.feature`:
"Reads alternate between two compute processes", "A closed compute process is skipped", "No compute process can be
reached", "A read is never resent after a frame", "A hung compute process answers code 17", "A compute process waiting
within the deadline is not hung", "A stream without credit holds off a hung compute's timer". Steps reworded, scenarios
kept: `client_order.feature` and `order.feature` (ComputeRead named), `role_read_steps.rs`' "the frontend sends the read
to compute only after the write is answered". `compute_tests` is untouched but for open question 1.

## 7. Open questions, each with a recommendation

1. **Merge order.** `run_read::merge` merges every partition's rows in key order through `Frontier`. §9.1 (since T38,
   T46, 2026-11-10), the ruling of 2026-11-18 (corner cases, *Settled*, item 1), T 5.5, L5.1 and L5.4 say partition by
   partition. The archived capability says key order: quad-reads, "Compute executes quad-pattern reads over storage", and
   its scenarios "A bound literal object fans out" and "An unbound subject merges every partition", which
   `compute_tests` binds; this change's delta does not modify that requirement. *Recommendation:* a MODIFIED delta of that
   requirement and its two scenarios to §9.1's order first; `merge` drains the partitions in ascending order, which drops
   `try_join_all` and the heap from it; the two `compute_tests` Then-steps follow, the one change to that crate, since
   `ComputeSession` shares `run_read`. The value-index lookup keeps key order: §9.1 governs Result batches only.
   `Frontier`, public and then unused, is removed only with your approval.
2. **The resume key's layout.** `ResumeKey` writes a 24-byte fixed part and three sections; §11.8 states a 32-byte fixed
   part with `partition` at 24 and a fourth section, the ordering value (since T48, 2026-11-10), and §4.1 still says
   `F = 24`. L5.5, C5.15, C5.19 to C5.21 and C5.26 test §11.8's layout. *Recommendation:* §4.1 corrected to 32 first;
   `ResumeKey`/`ResumeKeyView` in the wire crate take `partition` and the ordering value (empty: no family is ordered by an
   inline column before stage 6.5); the resume passes partitions below `partition`. The cost: a key written before this
   group is refused with Error 1 naming the resume key, and its client reads again from the start (§9.1). The pattern and
   scope bytes are equal (section 3), so nothing else of an old key differs. The alternative keeps the 24-byte key and
   moves those laws and cases to stage 6.5.
3. **Where the bound runs from.** A yield to the run queue gives "one step from the Commit's task being ready". From the
   bytes' arrival, compio polls the driver every 61 task polls (`event_interval`), so a read alone on its core may run up
   to 61 steps first. `sleep(Duration::ZERO)` completes at once in compio and is no yield to the driver.
   *Recommendation:* the yield to the run queue now, L5.9 over the scheduler as written; task 8.3 measures from arrival,
   and an endpoint core's `event_interval` is lowered, as a `[runtime]` setting with figures, only if that measure asks.
4. **"In the turn" for Credit and Cancel (L5.7, L5.8, C5.32).** A read builds its next batch only on room, so no batch
   ever waits on credit; on a Credit the read is woken in that turn and its frame leaves in its next step. On a Cancel,
   nothing more of the read reaches the client from that turn, and compio drops the task's future, and with it its Scans'
   Cancels, at the task's next slot of the core's pass. *Recommendation:* read the laws so; phase 2 asserts exactly that.
5. **`EndpointCore` and its `ComputeCore`.** *Recommendation:* the builder `EndpointCore::with_compute(Rc<ComputeCore>)`;
   a core holding none answers a read code 17 as one with no compute process does today. Making it an argument of `new`
   would change every caller's signature for no case production has.
6. **Scenarios task 5.3 names that cannot pass in group 5.** "Compute listens at no role address" is the switch's (6.2)
   and "Members names no compute node" needs Members for storage alone (group 1) and a compute node's Join (group 3);
   written now they fail under `fail_on_skipped`. *Recommendation:* move them to tasks 6.2 and 3.2.
7. **Tasks 5.6 and 5.7 must land together.** Once reads run in process, the scenarios of section 6's removal list fail:
   nothing sends a ComputeRead. *Recommendation:* your approval of task 5.7's removals before phase 4, so the group's gate
   is one green `make test`.
8. **An Execute's bound read past a frame.** In process no frame carries it, but the Read frame that section 3 builds
   bounds it at `MAX_FRAME_SIZE`. *Recommendation:* keep the refusal, code 6 with `detail` 1,048,576, and reword "An
   Execute too large for compute's frame is refused" to that frame.
9. **Testing the `Page::add_row` fix.** `Page` is crate-private and a page's size has no public observable short of 4 GiB
   of offsets. *Recommendation:* one in-src `#[cfg(test)]` module in `types/page.rs`, run by a new
   `rust_test(crate = ":ultragraphdb_common_compute")` target (`tdd-protocol.md`, "In-src test modules are declared to
   Bazel"), asserting the page's bytes after a record of one encoding and `k` statement IDs equal the encoding once. The
   fix: `Page::add_object(&mut self, encoding: &[u8])` and `add_row(&mut self, statement, property)` for rows of the last
   object, as `add_entry` and its rows already are. Phase 1 adds no `Page` surface until this is settled.
10. **The wire crate.** `ReadView::pattern_hash` and, with question 2, the key's layout are changes to
    `ultragraphdb_protocol_wire`, group 1's crate. *Recommendation:* make them in group 5: `ComputeReadView`, the rule's
    present home, is removed by task 1.5. The alternative, `common_compute` slicing the body with
    `ultragraphdb_core_body::Sections`, adds a dependency and a second reading of the layout.

## 8. Risks

- Two role connections per core and partition until group 4: storage sees twice the frontend's connections.
- `ComputeCore` dials the system group for Placement and Members on its own connection, so a core learns the map once
  more; the map is fixed in this stage.
- Partition order changes what a client of several partitions sees first: a test that read key order across partitions
  breaks; `compute_tests`' two scenarios are the ones found.
