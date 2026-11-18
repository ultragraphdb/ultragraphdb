# Group 0: `PROTOCOL.md` and the other specifications at two roles (tasks 0.2 and 0.3)

Nov 18, 2026. Specification text only. Edited: `specification/binary_protocol/PROTOCOL.md`, and one sentence each in
`specification/storage/STORAGE.md`, `specification/schema/SCHEMA.md` (two), `specification/query/QUERY-SPARQL-CORE.md`,
`specification/query/QUERY-GQL-CORE.md` and `specification/query/QUERY-ULTRA-QL.md`. Sections 1 to 8 are the first
pass. Section 9 holds the user's decisions on its six questions, with `openspec/notes/SECURITY.md` brought to two
roles, and section 10 the rulings on the fifteen points `notes/corner-cases.md` held open.

## 1. Verification

| Check | Result |
| --- | --- |
| `grep -n -i 'frontend\|ComputeRead\|from stage 5.9'` over `PROTOCOL.md` | no hit |
| The same over all of `specification/` | no hit |
| Section references: every `section N.M` of `PROTOCOL.md` that names this file resolves to a heading, and every `section N.M, *Name*` to a bold lead of that section | 105 headings, 887 references after section 10's edits, 0 unresolved. The script flagged a planted `section 13.99` and a planted `*Raeds*`, so it does find a broken one |
| Bazel targets reading `PROTOCOL.md` | none: `grep -rn PROTOCOL.md --include=BUILD.bazel .` finds nothing, and the golden frames come from the wire crate's generator. No target was run |

The script was `refs.py` in the session's scratchpad. It skips a reference that follows another file's name, and it
cannot tell a body's `section 1` from this file's section 1, so bare numbers 1 to 13 always resolve.

## 2. Sections of `PROTOCOL.md` changed

| Section | Change |
| --- | --- |
| Preamble | "Roles by stage" is now "Roles": two roles, compute holding the client endpoint and executing reads, one or more compute nodes. The reading rule for "From stage" paragraphs stays, for stages 6.6, 7 and 8. `SECURITY.md` is cited at `../../openspec/notes/SECURITY.md`, where it is |
| Contents | Item 11 without "and compute"; item 13 without ComputeRead |
| 1 | Tenant and Session rows: role name |
| 2.5 | Step 3 lists Nodes among what the side that dialled a client endpoint takes on stream 0. The paragraph after the table is gone |
| 3 | Rows 6, 8, 9, 20, 21, 26, 513 to 522, 526, 527, 531, 534 to 539: compute as sender or receiver. Rows 27 and 537 without their stage. Row 523 reads "retired". "523 is the one retired number". The closing paragraph names 540 alone as assigned by a later stage |
| 4.4 | Role name, three times |
| 4.7 | The row is "Nodes (27) and a Roster reply (537)" |
| 5.2, 5.4 | Role name; Nodes without "From stage 5.9" |
| 6.5 to 6.8, 7.4, 8.3 to 8.6 | Role name |
| 9.1 | `epoch` and `partition` name where the batch's rows were read; compute no longer "relays the batch" |
| 9.4 | Code 6 without the Execute's ComputeRead; code 17 without "no compute process answers a read, or a hung one"; references to 13.12 now name section 11.12, *Reads*; the retry paragraph says "another compute node" |
| 11.1, 11.2, 11.5, 11.9, 11.10 | Role name. "Connections a compute node holds at once" |
| 11.3 | The sentence on a ComputeRead stream's starting credit is gone |
| 11.8 | `pattern` and `scope` defined without ComputeRead (section 4, W2) |
| 11.11 | The client's read timeout rests on a partition's `scan_stall_ms`; the hung compute process is gone (question Q3) |
| 11.12 | Rewritten where the relay was: the opening paragraph, the role secret's usernames, the endpoint table, *Joining*, *Parked requests* (last two sentences), *Frames on their way* (the queue's owner and the frames counted), *Writes* (the first-request rule and code 23 of a read), *Reads*, *Errors*. *Reads* now holds what section 13.12 stated about compute's reads: the refusals of a read, Scans by level, Frontiers and the lane table, batches and `max_frame`, the corrupt row, the kernel table |
| 11.12a | Now "The catalog's offer, from stage 6.6": the stage 6.6 text alone, "compute" for "the role holding the client endpoint" |
| 11.13 | Role name; the lead *The frontend's reads* is *Compute's reads*, with the three references to it |
| 11.14 | Without "From stage 5.9"; a node with no open connection joins on the first it opens again (W4); "as before" removed |
| 13 (intro) | Kinds 2 and 3; kind 1 retired |
| 13.1 | One header table, one section table, one Order line (section 3 below) |
| 13.2 | Members is storage alone: `kind` 3, order by partition |
| 13.3, 13.7 to 13.11, 13.15, 13.16, 13.20, 13.23, 13.27, 13.28 | Role name; 13.16's connection role is `compute` |
| 13.4 | `nodes` in the header table and the Order line; a continuing frame's `nodes` among the subscriber's checks |
| 13.12 | "Retired (523)", two sentences. The number stays; nothing is renumbered |
| 13.13 | Role name; a transaction no longer held: compute holds the Error until the Bind stream ends, where it "relayed" it |
| 13.21 | The lane and version of each read: section 11.12, *Reads* |
| 13.26 | Without "From stage 5.9" |

## 3. The 17 "From stage 5.9" paragraphs

| Where it stood | Folded into |
| --- | --- |
| Preamble, the reading rule (2 mentions) | Preamble, *Roles* |
| 2.5, Nodes on stream 0 | 2.5 step 3 |
| 3, row 27 | The row, unqualified |
| 3, row 537 | The row, unqualified |
| 3, closing paragraph | Rows 513 to 539 and row 523 |
| 4.7, the Nodes and Roster row | The row, unqualified |
| 5.4, first words | Removed |
| 11.12a, *Compute holds the client endpoint*, with *Reads*, *A transaction's order*, *Connections* | 11.12: opening paragraph, role secret, *Joining*, *Reads*; 13 intro for role kind 1 |
| 11.12a, the endpoint table | 11.12, *What each endpoint serves* |
| 11.14, first words | Removed |
| 13 intro, role kind 1 | 13 intro |
| 13.1, the fields added | 13.1's tables |
| 13.1, "The order from stage 5.9, whole" | 13.1's Order line |
| 13.2, Members for storage alone | 13.2's member table and Order line |
| 13.4, `nodes` | 13.4's header table and Order line |
| 13.26, first words | Removed |

Of section 11.12a, the catalog's offer and the first schema's fence are stage 6.6's and stay there.

## 4. Weaknesses fixed

- **W1. Section 13.12 was the only statement of how compute reads.** The errors of a read, the `at` of each Scan by
  level, the Frontier lanes, the batch rules and the kernel table were ComputeRead's section. They now stand in section
  11.12, *Reads*, unchanged in substance, and every reference to 13.12 for them names that paragraph.
- **W2. The resume key was defined by ComputeRead's bytes.** `pattern` hashed "ComputeRead's sections 0 to 2" and
  `scope` its `transaction` or `read_version`. Section 11.8 now states the bytes: the bound pattern (32 bytes), the
  term table's records, the table's text, with nothing between, which for a Read are its own sections 0 to 2. Those
  are the bytes the code hashes today, since the records end on a multiple of 8. `scope` is the transaction's identity
  as its Bind names it, or the version a read-only transaction reads at. See question Q4 for an Execute.
- **W3. *Parked requests* said "a Scan and its Credit travel on compute's connection, which parks nothing".** With one
  connection per core and partition (design D2, former section 11.12a *Connections*) that is false. The text now lists
  a Scan, a Frontier and a Credit among what waits on a stopped connection. See question Q1.
- **W4. A compute node with no open connection to the system group.** Section 11.14 said where it joins again "while
  another of its connections is open" and nothing for the case of none. It now joins on the first connection it opens
  again, which K46's grace of twice `epoch_ms_max` assumes.
- **W5. *Joining* had compute ask Members for its own role address** to decide whether to join again. It has none.
  The paragraph states storage's rule and the node list's apart: the list is logged and survives a restart of the group.
- **W6. Code 23 of a read and a held read** were written as an exchange between two processes ("when compute answers
  … the frontend cancels", "sends none of its later requests … to compute"). They are now one role's rules.
- **W7. `SECURITY.md` "beside this file".** It is at `openspec/notes/SECURITY.md`; the preamble now gives that path.
- **W8. Section 13.1's last sentence, "A Join is answered once it is durable in the metadata state machine",** stood
  in the stage 5.9 paragraph and is scoped to a compute node's Join. See question Q5.

## 5. Questions for the user

All six are decided; section 9 holds each decision.

- **Q1. Reads now wait behind a partition that stopped reading a connection.** A partition stops reading a role
  connection that holds `storage.held_bytes` parked. Before the merge reads had a connection of their own. With one
  connection per core and partition, every read of that core to that partition waits too, other clients' reads
  included, for at most `transaction_ms`. *Written:* that they wait. Is this accepted, or do reads keep a second
  connection per core and partition?
- **Q2. Do a read's Scan, Frontier, Closed and Credit frames count against `endpoint.held_bytes`?** The relayed
  ComputeRead and Credit did. *Written:* they count, which keeps the stated bound of connections times `held_bytes`.
- **Q3. Who answers a read when a compute node or a partition hangs?** The frontend answered code 17 at the deadline
  plus `scan_stall_ms`, and the Rust client's timeout floor of 30 s was sized to that. That timer is gone. A hung
  partition is answered at the deadline plus twice `scan_stall_ms`, up to 60 s, which a 30 s client timeout can
  precede. *Written:* the client's timeout rests on a partition keeping a stalled Scan for at most `scan_stall_ms`.
  Should the floor rise to twice `scan_stall_ms`? The wire-protocol delta ("The client's timeout leaves a read its
  deadline") still says "a hung compute node is answered with code 17", which nothing does now; it needs the same
  decision.
- **Q4. The resume key's `pattern` for an Execute.** Its hash covers the term table compute builds when it binds the
  parameters, and the order of that table is stated nowhere. With several nodes, a key written by one node is read by
  another, so the order must be a rule. What is it: the statement's table as prepared, then each parameter's term in
  parameter order?
- **Q5. Is a storage process's Join logged?** *Joining* implies not ("a restarted group, which holds no Join from
  before"), while the placement its Join claims is durable. Section 13.1 now says only that a compute node's Join is
  answered once durable.
- **Q6. `SECURITY.md` lies outside `specification/`.** See section 6.

## 6. `SECURITY.md` (task 0.3)

The file is `openspec/notes/SECURITY.md`, not under `specification/`, and this task's edits were limited to
`specification/`. The text below was applied once the user allowed the edit (section 9, Q6).

*Tiers*, first sentence: "A cluster is two role processes, compute and storage, and the config service."

The table's rows for the frontend and compute become one:

| Tier | Holds | A compromise exposes |
| --- | --- | --- |
| Compute | The client endpoint, its TLS key, the role secret, in memory the counts of failed authentications, the throttle and the ban, and the rows of the reads it executes. Nothing durable, no store and no data directory; no principal's verifier; it accepts no role connection | The frames of the clients it serves, the rows of their reads, and the role secret, which opens a role connection to every partition and to the system group. No stored key or server key, so no verifier to guess passwords against |

Elsewhere "the frontend" reads "compute" with no other change at lines 58 ("every compute node"), 79, 85, 155, 182,
243, 328, 330 and 351. Three places need more than the name:

- Line 190: "Compute and storage authenticate each other …"; line 192: "Compute dials its role connections and
  accepts none."
- Line 246: "compute, which holds no store, passes the principal on in each Scan" stays true as written.
- *Where it is built*, rows "Stage 5.5, the frontend" and "Stage 5.8, the system group": a record by stage. Either the
  rows keep the stage's own role name, or they are renamed and a row "Stage 5.9" added. The user's choice.

## 7. Other specification documents naming the frontend or ComputeRead

Each was a role name in a statement that holds for compute, and each is edited.

| Place | Was | Now |
| --- | --- | --- |
| `STORAGE.md` 18.1 | "the role holding the client endpoint (the frontend until stage 5.9, compute from it) and the planner choose" | "compute and the planner choose" |
| `SCHEMA.md` 9.6 (line 1388) | the same parenthesis | "the facts compute decides a request's path from" |
| `SCHEMA.md` decisions table (line 2230) | "has the frontend assemble an instance to route it" | "has compute assemble …" |
| `QUERY-SPARQL-CORE.md` (line 363) | "the role holding the client endpoint, the frontend until stage 5.9 and compute from it, decides" | "compute decides" |
| `QUERY-GQL-CORE.md` (line 431) | the same | "compute decides" |
| `QUERY-ULTRA-QL.md` 23, an open question | "a feed the frontend assembles" | "a feed compute assembles" |

No other document under `specification/` names either.

## 8. For the groups that follow

- Group 1 (the wire): Members' Order line is shorter. A member's `kind` is 3, and "member order" is the partitions
  ascending. EpochClose's unused bytes are 18 to 23 and 32 to 127.
- Group 5: the resume key's `pattern` is no longer `ComputeReadView::pattern_hash`; section 11.8 gives the bytes.
- `PROTOCOL.md` line 11 still names stage 5.5's group 8 as what brings the crates to the file. It was left.

## 9. The six questions, decided 2026-11-18

| Question | Decision | What was done |
| --- | --- | --- |
| Q1, reads behind a stopped connection | They wait; one connection (design D2) | The text stands |
| Q2, `endpoint.held_bytes` | A read's Scan, Frontier, Closed and Credit count | The text stands |
| Q3, the client's read timeout | The floor rises to cover compute's worst case | Below |
| Q4, an Execute's term table | Stated as a rule from the code | Below |
| Q5, storage's Join | As scoped | The text stands |
| Q6, `SECURITY.md` | Applied at `openspec/notes/SECURITY.md` | Below |

**Q3.** Section 11.11: the Rust client's timeout for a read runs from the later of its last progress and the Begin's
deadline and is at least twice the most `scan_stall_ms` holds and 2,000 ms more, 62,000 ms. The derivation: compute
answers code 17 for a hung partition at the request's deadline plus twice `scan_stall_ms` (section 11.12, *Errors*);
`scan_stall_ms` is at most 30,000 ms (section 11.9); the 2,000 ms is the room section 11.9 leaves a reply within the
client's default timeout, `catalog_check_ms` 8,000 against 10 s, since section 11.11 named no margin of its own. The
limits table's `scan_stall_ms` row says the client's timeout "leaves twice it room". The wire-protocol delta's
requirement "The client's timeout leaves a read its deadline" states the same floor and gains the scenario "A hung
partition is heard as code 17". `openspec validate stage-5-9-compute-role --strict` passes.

For group 7, the Rust client changes in `ultragraphdb_client_core`, `src/types/client_core/mod.rs`: the constant
`READ_TIMEOUT_MIN` from 30 s to 62 s, with its comment, which still names a hung compute process and the deadline plus
`scan_stall_ms`; and the doc comment of `ClientConfig::with_timeout` in `src/types/client_config.rs`, "A read waits at
least 30 s". The config-service delta's Bounds row calls 30,000 "the floor of the client's read timeout"; the bound on
`scan_stall_ms` stays 30,000, and the row's reason now reads wrongly. It was not edited.

**Q4.** The code is deterministic (`ultragraphdb_protocol_wire`, `StatementCache::execute` and `TermTable::import`),
and section 11.8 states it: the statement's table as its Prepare sent it, then the parameters' terms in parameter
order, each taken from the Execute's table with a triple term's subject, predicate and object before it, a term the
table already holds keeping its index; the pattern is the statement's with each parameter slot bound to that index.
The `HashMap` in `import` is a lookup alone and orders nothing.

**Q6.** `openspec/notes/SECURITY.md`: *Tiers* names two role processes and has one Compute row for the frontend's and
compute's; "compute" for the frontend in the sections on principals, authentication, throttling, role connections,
enforcement, sessions and TLS. In *Where it is built* the rows "Stage 5.5, the frontend" and "Stage 5.8, the system
group" keep their stage's role names, and a row "Stage 5.9, two roles" is added before stage 7's.

## 10. The fifteen open points, ruled 2026-11-18

`notes/corner-cases.md` now holds them under "Settled in the specification (2026-11-18)", with one left under "Open
in the specification".

| Item | Where closed | Text |
| --- | --- | --- |
| 1, merge order | P §9.1, already | "no order holds by term or by key (T38, T46)". Nothing added |
| 2, resume key hashes | P §11.8 | Covers `pattern` and `scope`, both without ComputeRead |
| 3, advertised host | P §5.4 | One encoding: a DNS name of `a` to `z`, digits, `-` and `.`; an IPv4 literal of four numbers without a leading zero; an IPv6 literal in RFC 5952's form |
| 4, Join with no connection open | P §11.14; §11.12, *Joining* | Already written in the first pass (W4) |
| 5, Join refused with code 15 | P §13.1 | The node logs the bound and exits with code 78 |
| 6, drain | P §11.14, *Draining*; node-discovery, "A draining node is announced before it stops" | `transaction_ms` from the start of the drain wins |
| 7, client delays | P §11.14, *Seeds* and *What a client does with the list* | 0 to 1,000 ms before dialling a new node; backoff from the retry delay, 50 ms, doubling to 30,000 ms, each wait from half the delay to the whole |
| 8, "unacknowledged by the transport" | P §13.3 | A full send buffer on either carrier, or on TCP the user timeout on unacknowledged bytes |
| 9, wildcard host | P §5.4 | `0.0.0.0` and `[::]` refused |
| 10, tag count | P §13.1 | `CountMismatch` `"tags"`, against 1 for none and 8 for more |
| 11, both bounds | P §13.1 | The nodes' bound first: `detail` 1,024 |
| 12, a Join's `kind` | P §13.1 | The connection's role, else `FieldValue` `"kind"`, checked by the system group's session |
| 13, `nodes` | P §11.14, §13.4 | 1 when any command changed the list, and the version rises |
| 14, Begin at a draining node | open | Below |
| 15, Nodes flags | P §5.4 | `flags` 0; a Nodes flagged `LAST` refused with `FieldValue` `"flags"` |

**Item 1, the sentences to correct.** `tasks.md` 5.5: "the merge returns the rows of any drawn partitions' sorted
entries in key order, each once, whatever the drawn boundaries of the Entries frames and of the client's frames".
`design.md` D11: "the executor's merge returning the rows of any drawn partitions in key order whatever the frame
boundaries".

**Item 7, proposed figures.** Neither the document nor `ultragraphdb_client_core` held a random delay, a factor, a cap
or a jitter. The client holds `retry_delay`, 50 ms, a fixed delay between a commit's attempts, which is taken as the
backoff's first delay. The 1,000 ms, the doubling, the 30,000 ms cap and the draw from half the delay are proposed:
the doubling and the cap are those of the authentication throttle (section 11.9).

**Item 8, the reading taken.** Stage 5.8's sequencer requirement holds the same words, "unacknowledged by the
transport", and no observation. A full send buffer alone holds for both carriers but does not close a dead machine's
connection within twice `epoch_ms_max`, which system-group's scenario "A silent node's connection is closed" asks, so
TCP takes the user timeout as well. A Unix socket's peer closes with its process.

**Item 14, not written.** No code says "retry on another node", and section 11.14 names none. The candidates:

- Code 17 (Unavailable). Its row makes it retryable on a read alone, and on a Begin "no"; the row would gain the Begin
  at a draining node, retried on another node.
- Code 5 (Busy), retryable, with a new `detail` 4 for a draining node. Its `detail` values 1 to 3 are storage's
  admission.
- A new code 33 (Draining), retryable on another node, the one reading a driver cannot mistake.

**Stated on the other reading, corrected.** `notes/laws.md`: L7.14 held the stop to "no earlier than" the Nodes
without a bound and to `transaction_ms` after the latest transaction began; it is written on the drain's start. L7.10
takes the stated bounds, and L1.8's generator draws the forms item 3 refuses. `notes/corner-cases.md`: C1.73, C3.11,
C3.13, C3.21, C3.23, C3.44, C3.45, C7.6, C7.17, C7.21 and C7.40 to C7.42 state the ruled outcome. The cases items 3 and
9 add are listed in the settled entries and not yet numbered among group 1's, so that note's count of 84 stands.

**Section 13.1's intro** names a role address for a storage process alone.

**Left for their owners.** sequencer, "A close names a change of the node list" ("whether the node list changed since
the previous close") and node-discovery's client requirement ("after a random delay", "exponential backoff and
jitter") agree with the rulings and state no figure. config-service's Addresses rule does not yet refuse a wildcard
host in `compute.advertised_address`, which P §5.4 now refuses at Join; L2.5 needs the two to agree.

## 11. The last batch, 2026-11-18

**Code 33, Draining (item 14).** Code 33 was free: the error table ended at 32, TagRequired, which is stage 7's code
for tags (sections 9.4 and 11.14). Added:

- Section 9.4: the row of code 33, retryable at once on another compute node, `detail` 0; and its line in the retry
  paragraph.
- Section 11.14, *Draining*: from the moment it begins to drain, a node refuses a Begin, a DirectWrite, a
  ConditionalWrite, an Ingest and a Drop with code 33, writing nothing, and serves every request of a transaction
  already open. Those five are the requests that start work; a Drop is among them because it commits in one frame.
- The client's view. Section 11.14 says a client "never edits its list itself", so the list is left alone. The client
  holds a mark beside it: the refusing node gets no new work until a list of a higher version names it serving. With
  no other serving node of its tag, the client returns code 33 to its caller.
- No section lists codes per command: section 3's "Answered by" says "or Error" alone, so nothing else changed.
- node-discovery, "A draining node is announced before it stops": the rule, and the scenario "A draining node refuses
  new work with code 33".
- `notes/corner-cases.md`: item 14 is settled and the open section is gone; C7.43 states the outcome, and C7.48 to
  C7.51 are added, group 7 now 51 cases. `notes/laws.md`: L7.15, group 7 now 15 laws.

For group 1 the wire gains error code 33, and for group 7 the client core the mark and the retry.

**Accepted.** The margin of 2,000 ms in the client's timeout floor, and the delay and backoff figures of item 7.

**config-service delta.** The Bounds row guards the client's floor: the floor of 62,000 ms is twice the most
`scan_stall_ms` holds and a margin, so a larger `scan_stall_ms` would let the client's timeout precede compute's code
17. The bound stays 30,000 and the row gives that reason; its scenario is "A stall past what the client's read timeout
covers is refused". The advertised address's form is the Compute row's, not the Addresses row's, so the Compute row
states that section 5.4's one encoding refuses a wildcard host, an upper-case letter and a non-canonical IP literal,
and the scenario "An advertised address a Join would refuse is refused" is added (law L2.5).

**"In key order".** `tasks.md` 5.5 and `design.md` D11 state section 9.1's rule: partition by partition, in ascending
order of the partitions, each partition's entries in their own order.

**Checks.** `openspec validate stage-5-9-compute-role --strict` passes. `PROTOCOL.md`: 105 headings, 888 references,
0 unresolved; the grep for `frontend`, `ComputeRead` and "from stage 5.9" finds nothing.
