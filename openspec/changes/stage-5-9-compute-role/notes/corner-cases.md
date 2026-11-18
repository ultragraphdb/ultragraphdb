# Stage 5.9: the corner cases of every group

Task 0.5. The degenerate and boundary inputs of each group, enumerated before its phase 2
(`openspec/protocols/tdd-protocol.md`, "The suite covers corner cases enumerated in advance"). Each case is a unit test
of its group's phase 2, or a run over the built binaries where its section says so. The group's `notes/group-N.md`
names, by the case's number here, the test that covers it, when its phase-2 exit is claimed.

Sources are cited as `P §n` for `specification/binary_protocol/PROTOCOL.md`, as `capability, "requirement"` for the
change's `specs/`, as `D n` for `design.md`, as `T n` for `tasks.md` and as `F §4.n` for case n of
`notes/feasibility.md` section 4. `PROTOCOL.md` is cited by section number, with the "From stage 5.9" paragraphs read
as part of the sections they amend (T 0.2). Every outcome is the specification's. A case whose outcome no text states
reads "not stated" with its item under *Open in the specification*.

A decoder's cases stand in the order of its body's "Order" line. An unused byte is refused as `ReservedNotZero` naming
its frame offset, 128 plus the command-header offset (P §4.2). "Read" means the view accepts the body and gives the
fields back. `E` is `epoch_ms_max`, `T` is `transaction_ms`, `G` is the grace, twice `E`.

The groups stand in the order they land (D 12).

| Group | Cases |
|---|---|
| 5, the endpoint executes reads | 52 |
| 6, the switch | 36 |
| 4, one connection per core and partition | 22 |
| 1, the wire | 84 |
| 2, settings of a node | 17 |
| 3, the node list | 55 |
| 7, several compute nodes and the clients | 51 |
| 8, the cluster end to end | 13 |
| 9, the benchmark | 3 |

## 5. The endpoint executes reads, `ultragraphdb_common_compute`

Each case drives the executor's message seam (D 3) with played partitions, but C5.50 to C5.52, which run over the
harnesses of `ultragraphdb_frontend_tests` and `ultragraphdb_tests`.

### The merge and the client's frame

Sources: P §9.1, §11.12, *Reads*; quad-reads, "A read's batches fit the client's frame".

| Case | Input | Outcome |
|---|---|---|
| C5.1 | a read matching nothing in any partition | one batch flagged `LAST`, `rows` 0, `columns` the unbound pattern slots and one more, no resume key |
| C5.2 | one partition, one row | one batch flagged `LAST` holding the row |
| C5.3 | four partitions, the second and the fourth holding no row | the rows of the first, then of the third, each in its partition's order |
| C5.4 | a partition's entries cut into Entries frames of one entry each; the same entries in one frame | the same rows in the same order from both |
| C5.5 | a client whose Hello names `max_frame` 524,288, and one Entries frame of 1,048,576 bytes full of rows | two or more Result frames, each at most 524,288 bytes, every row once; one Result frame is built per step |
| C5.6 | a client whose Hello names `max_frame` 1,048,576 | every Result frame at most 1,048,576 bytes |
| C5.7 | a row whose terms fill an otherwise empty batch of the client's `max_frame` exactly; one byte more | the first is sent alone in its batch; the second refuses the read with code 6, `detail` the client's `max_frame` |
| C5.8 | at `max_frame` 524,288, a row holding a literal of 600,000 bytes, as the first row and after 10 rows | code 6, `detail` 524,288, in both; no row after it is sent |
| C5.9 | a read stopped at its `limit` whose rows fill the last batch so its resume key does not fit | the stream's last batch holds `rows` 0 and the key and spends one credit; the batch before it is not flagged `LAST` |
| C5.10 | a resume key too large for a batch of `rows` 0 within the client's `max_frame` | the read is refused with code 6 |
| C5.11 | a read stopped at its `limit` exactly at the last row that matches | the last batch holds a resume key, since the partition reads no further to tell; the resumed read answers one batch flagged `LAST`, `rows` 0, no key |
| C5.12 | `limit` 1 with rows remaining; `limit` equal to the rows that match less one | each stream's last batch holds a key; the pages concatenate to the whole answer |

### Resume keys

Source: P §11.8. C5.17 to C5.25 stand in the order of its checks; each is refused with Error 1 (Protocol) on the
request's stream, the message naming the resume key.

| Case | Input | Outcome |
|---|---|---|
| C5.13 | a read resumed from the key of its first row | exactly the rows after the first |
| C5.14 | a read resumed from the key of its last row | one batch flagged `LAST`, `rows` 0, no key |
| C5.15 | a key of the last row of partition 2, partitions 1 to 4 asked | no row of partitions 1 and 2; partitions 3 and 4 read from their start |
| C5.16 | a key whose last row was removed since, at levels 1 and 2 | the read continues strictly after that row's place |
| C5.17 | one bit of the key changed | refused: the checksum |
| C5.18 | a key with a right checksum over a body the layout of P §4.1 refuses | refused: the body layout |
| C5.19 | byte 28 nonzero; byte 31 nonzero | refused: `PaddingNotZero` |
| C5.20 | `partition` 0 | refused: `FieldValue` `"partition"` |
| C5.21 | section 0 of 16 and of 24 bytes | refused: `CountMismatch` `"resume row"` |
| C5.22 | a term table P §4.4 refuses | refused by the table's rule |
| C5.23 | an index at the table's length; `ABSENT` at the subject, the predicate or the object | refused: `IndexOutOfRange` |
| C5.24 | a key of another pattern, its checksum right | refused: the pattern hash |
| C5.25 | a key of another read-write transaction; of another `read_version` | refused: the `scope` |
| C5.26 | byte 28 nonzero and `partition` 0 in one key, its checksum right | refused as `PaddingNotZero`, the earlier check |
| C5.27 | `ABSENT` at the graph and at the reifier | read: the default graph, no reifier |
| C5.28 | a key naming a version below a partition's fold horizon | code 3 (TooOld) |

### Credit

Sources: P §9.2, §11.3; quad-reads, "A read's batches fit the client's frame"; D 4.

| Case | Input | Outcome |
|---|---|---|
| C5.29 | `initial_credit` 4 and an answer of six frames | four frames are sent, then one for each frame of the client's Credit |
| C5.30 | `initial_credit` 1 | one frame, then one per frame of credit |
| C5.31 | a stream's credit at 0 with frames ready | no Result frame is sent until a Credit arrives |
| C5.32 | a Credit read in the turn of the step that built a frame the spent credit held back | the frame is sent in that turn; read as L5.7 is (`notes/laws.md`, below L5.14): a read builds no frame credit holds back, the Credit wakes it in that turn and the frame leaves in its next step |
| C5.33 | two Credits of `frames` 2³² − 1 | the credit saturates at 2³² − 1 |
| C5.34 | a Credit of `frames` 0 | refused with Error 1 on its stream, `FieldValue` `"frames"`, which ends the read |
| C5.35 | a Credit for a stream that is not open; after the read's `LAST` | ignored |
| C5.36 | a Credit while the read holds a Write back | compute reads the Credit, the read reaches its `LAST`, and the held Write is then sent |

### Cancel

Sources: P §11.4; quad-reads, "A read through compute is a read of its transaction", "A read's batches fit the client's
frame".

| Case | Input | Outcome |
|---|---|---|
| C5.37 | a Cancel of a read still held behind a write | the read is dropped unsent: no Scan leaves |
| C5.38 | a Cancel between two Result frames, Scans open at two partitions | both Scans are cancelled in the turn the Cancel is read; no further Result frame is sent |
| C5.39 | a Cancel read in the turn of a step that built a frame not yet written | the frame is dropped; nothing further is sent on the stream |
| C5.40 | a Cancel after the read's `LAST` | a late frame: checked and dropped, the connection open |
| C5.41 | a Cancel, then a Credit for the same stream | the Credit is ignored |
| C5.42 | a Cancel on a transaction's stream with two reads open | the transaction aborts and both reads end |
| C5.43 | a Cancel of a read that holds a Write back | the Write is sent; the transaction goes on |

### The step

Sources: quad-reads, "A read's step is bounded by one frame"; D 3.

| Case | Input | Outcome |
|---|---|---|
| C5.44 | a read merging four partitions under way, and a Commit of another connection arriving on the core | the Commit is relayed before the read's next Result frame is built |
| C5.45 | a read whose partitions each have ten Entries frames ready | it consumes one frame, yields, and consumes the next only after every other ready task of the core has run a step |
| C5.46 | a read alone on its core | it runs step after step, giving the core back between each two |
| C5.47 | a reverse read over an object's referrers, ten Neighbours frames ready | one frame a step, as C5.45 |

### A transaction's order, errors and the processes

Sources: quad-reads, "A read through compute is a read of its transaction", "Compute times a partition that stops
answering"; client-endpoint, "Errors reach the client through the error table"; T 5.6.

| Case | Input | Outcome |
|---|---|---|
| C5.48 | a read under way when its transaction ended, the Scan's Error 1 of `detail` 1 arriving before the Bind stream's Error | compute holds the Scan's Error until the Bind stream ends, then answers the read with the Error the stream gave; a read already answered takes nothing further |
| C5.49 | a partition's connection closing under a read, before any frame reached the client and after one did | code 17 in both; no Scan of the read is sent again, and no other process is tried |
| C5.50 | a partition accepting a Scan of `deadline_ms` 50 and sending nothing, `scan_stall_ms` 100 | compute closes the connection once 250 ms have passed and not before, and answers the read code 17; frames with gaps below 200 ms past the deadline keep the read open to its `LAST` |
| C5.51 | a cluster of the harnesses run once with a compute process and once without | every read feature is green in both, its scenarios unchanged (T 5.6) |
| C5.52 | the frontend process killed and restarted during a paged read | the client's connection closes; the read resumed from its last key on a new connection, in a read-only transaction at level 0 naming the first Begin's version, returns the rows after that key |

## 6. The switch

### The Roles check, the retired names and another role's table, `ultragraphdb_common_config_check`

Sources: config-service, "The config service validates every setting before it serves one", "A configuration error
tells the administrator what to do"; T 0.1, T 6.3.

| Case | Input | Outcome |
|---|---|---|
| C6.1 | a target with storage instances and no compute instance | refused by the Roles rule, naming the target |
| C6.2 | a target with compute instances and no storage instance | refused by the Roles rule, naming the target |
| C6.3 | one compute and one storage instance | stored |
| C6.4 | `compute_1`, `compute_2` and `storage_1` | stored (D 8) |
| C6.5 | any file holding a `[frontend]` table | refused naming the table as unknown |
| C6.6 | a compute instance's file holding `compute.role_address` | refused naming the setting as unknown |
| C6.7 | any file holding `cluster.frontend_address` | refused naming the setting as unknown |
| C6.8 | an instance named `frontend_1` | refused by the Names rule: the name begins with no role's name |
| C6.9 | a compute instance's file holding a `[storage]` table; a storage instance's holding a `[compute]` table | refused naming the table and the instance |
| C6.10 | a compute instance holding a `[metadata]` or a `[sequencer]` table | refused by the System rule |
| C6.11 | one revision holding C6.5, C6.8 and C6.9 | refused listing all three, each naming its instance and setting |
| C6.12 | the config service started over a directory whose active revision holds a `[frontend]` table | exit code 78 naming the target, the revision and the table; the release before, over the same directory, serves as it did |

### The system group's table and the role names, `ultragraphdb_common_system`, `ultragraphdb_common_role`

Sources: P §2.5 step 3, §11.12a; system-group, "The system group serves the books' role commands at its role address";
wire-protocol, "The node list's frames are bodies of the protocol".

| Case | Input | Outcome |
|---|---|---|
| C6.13 | a connection authenticated as `compute` sending each of Join, Members, Placement, Catalog, Subscribe, CloseRequest, Challenge, Verify, Session, Check, Ended, Declare, Defaults | each is served |
| C6.14 | a connection authenticated as `compute` sending Lease, DefaultEntry or Report | `CommandNotServed` naming the command |
| C6.15 | a connection authenticated as `storage` sending Challenge, Verify, Session, Check, Ended, Declare or Defaults | `CommandNotServed` naming the command |
| C6.16 | either role sending the system group a Bind | `CommandNotServed` naming Bind |
| C6.17 | a role connection authenticating under the name `frontend` | refused as a wrong secret is; it sends no role command |
| C6.18 | a ComputeRead (523) sent to the system group and to a partition, after task 6.4 and before task 1.5 | `CommandNotServed` naming it, while the wire still holds the number; `UnknownCommand` once task 1.5 has removed it |
| C6.19 | a Roster (537) sent to the system group before group 1 | `UnknownCommand`: the number becomes a command with task 1.2 and is served with task 3.3 |

T 6.6: `Server::machine_of` through a clock the shell takes. Source: P §11.4, §11.9, §11.12.

| Case | Input | Outcome |
|---|---|---|
| C6.20 | a client connection sending no frame for its idle limit, 600 s, exactly, and for one tick less; a role connection silent for 600 s and for ten times that | the client connection is closed at the limit and open one tick before it; the role connection stays open, since it has no idle limit |

### The compute binary, `ultragraphdb_compute`, `ultragraphdb_compute_tests`

Sources: role-binaries, "Readiness, health and shutdown", "Roles meet through the transport at their addresses", "Each
role is its own binary and image", "A role takes a port a killed process leaves held"; client-endpoint, "Compute finds
the partitions in Members"; system-group, "A lost system group is waited for until the deadline"; T 6.3.

| Case | Run | Outcome |
|---|---|---|
| C6.21 | compute started | it listens at its client address and its observability address alone |
| C6.22 | compute started while another process holds its client address, which lets it go within 5 s | `/health` answers 503 while the address is held; once it answers 200 a client connection is accepted at once |
| C6.23 | compute started while the hosting storage process still recovers; started before any process listens at `cluster.system_address` | `/health` answers; `/ready` answers only after storage's, once the group answers, the view holds a placement and a close, Members lists every partition and a partition answers |
| C6.24 | the hosting storage process killed and not back within `join_deadline_ms` | compute shuts down through its ordinary path and exits 69 naming the system group's address |
| C6.25 | SIGTERM with no transaction open | compute stops accepting connections, finishes its open requests and exits 0 |
| C6.26 | SIGTERM while a client holds a read-write transaction open and sends nothing more | compute stops no later than `T` after the transaction began |
| C6.27 | SIGTERM while a read-only transaction is open | the transaction ends with its connection when the node stops |
| C6.28 | a compute process between the switch and task 3.7 | it sends no Join; Members holds storage members alone; nothing dials it |
| C6.29 | the compute binary's dependency graph listed | it names neither `ultragraphdb_store_kv`, fjall, `ultragraphdb_common_system`, `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` nor `ultragraphdb_transactions_log` |

### The supervisor, the image and the chart

Sources: role-binaries, "The supervisor starts, stops and reports the roles", "Configuration is explicit and validated
at start"; kubernetes-deployment, every modified requirement; T 6.3, T 6.8.

| Case | Run | Outcome |
|---|---|---|
| C6.30 | the supervisor starting a target of two roles | the config service answers first, storage's `/ready` answers before compute is started, compute's answers last |
| C6.31 | the storage process exiting with code 74; compute killed by SIGKILL | the supervisor stops the other role and exits 74; 137 |
| C6.32 | SIGTERM to the supervisor | compute stops accepting work and drains first, storage flushes last, exit 0 |
| C6.33 | a target whose storage file names another `role_secret_file` than its compute file's | exit 78 naming both files; no role started |
| C6.34 | the development image booted, once per architecture, amd64 under emulation | each role reaches `/ready`; `/data` holds `system/meta/`, `system/seq/` and `storage/p1/r1/` and no directory of compute's |
| C6.35 | the workspace's members, the images, the chart and the profiles searched for `frontend` | none found |
| C6.36 | the chart rendered with two compute instances | each is a StatefulSet of one replica with no volume under a headless Service; one Service spans every compute pod; a compute pod mounts the role secret and no bootstrap secret |

## 4. One connection per core and partition, `ultragraphdb_common_compute`

Each case drives `RoleLink` against the played partition of T 4.2. C4.1 to C4.8 are the five cases of
`notes/feasibility.md` section 4.

| Case | Input | Outcome | Source |
|---|---|---|---|
| C4.1 | a Scan whose frame cannot be built | no stream is registered for it; the read is answered its refusal; a frame arriving later on that number is a stray frame | F §4.1; T 4.2 |
| C4.2 | a Scan whose frame cannot be sent, the link having closed beneath the write | no stream is registered; the read is answered code 17 | F §4.1; client-endpoint, "Errors reach the client through the error table" |
| C4.3 | a write that fails with a Bind stream and two Scans open on the link | the link is marked closed at once; each of the three streams is answered code 17, once | F §4.2 |
| C4.4 | the next request after C4.3 | the link is dialled again and the request is sent on the new connection; it is not answered code 17 for the old one | F §4.2; P §11.12, *Errors* ("dials again for the next read") |
| C4.5 | the tap granting one frame more than compute's credit, a second read's Scan open on the link | compute refuses the frame as `PastCredit` naming the stream, sends no Error, closes the link and answers both reads code 17 | F §4.3; wire-protocol, "A frame past its stream's credit is refused" |
| C4.6 | Entries frames up to exactly the stream's credit | each is taken; the link stays open | P §11.3 |
| C4.7 | a frame on a stream whose reader is gone: whole and valid; with one chunk's checksum broken | the first is checked whole and dropped, the link open; the second is refused as `Chunk` and closes the link | F §4.4; P §2.5 step 3, steps 11 and 12 |
| C4.8 | three played partitions bound to ports in the order 3, 1, 2, the read's subject listing them 1, 2, 3 | each Scan reaches the partition its subject names; no assertion reads a double by the order it bound | F §4.5; T 4.2 |
| C4.9 | an Entries frame on a stream that carries a Bind and no Scan | the link is closed, as for a Bind's stray reply; each stream it carried is answered code 17 | T 4.2; P §2.5 step 3 |
| C4.10 | a Held on a Scan's stream; a Neighbours frame on a Scan's stream; an Entries frame on a Frontier's stream | each closes the link: the frame answers no request of its stream | P §2.5 step 3, §3 ("Answered by") |
| C4.11 | a frame on a stream numbered at or above the next stream the link will open | refused; the link is closed | P §2.5 step 3 |
| C4.12 | an Error on a Scan's stream; an Error on stream 0 | the first ends that read with the Error's code, the link open; the second is the link's refusal and closes it | P §2.6, §11.12, *Errors*; T 0.7 |
| C4.13 | a Scan cancelled, and its Entries already on their way arriving after | each is a late frame: checked and dropped | P §2.5 step 3, §11.2 |
| C4.14 | one link carrying connection A's Commit and connection B's Scan, B's Scan answered past its credit | the link closes; A's Commit is answered code 17 and is in doubt; B's read is answered code 17 | D 2; wire-protocol, "A commit in flight is in doubt once sent" |
| C4.15 | a partition leaving a Scan unanswered past `deadline_ms` and twice `scan_stall_ms`, a Bind stream of another transaction open on the link | compute closes the link; the read and the Bind stream's requests are each answered code 17 | quad-reads, "Compute times a partition that stops answering"; D 2 |
| C4.16 | a read-write transaction writing and then reading | the Bind stream and the Scan leave on one link; the Scan leaves after the write's Held, and names the transaction | quad-reads, "Compute runs each read on one core"; D 2 |
| C4.17 | a Scan's stream at its start | it holds the `initial_credit` of the partition's Welcome; compute grants more as it drains that partition's answer | P §11.3, §11.12, *Reads* |
| C4.18 | a Closed request | answered by Closed on its stream; the stream ends | P §13.17 |
| C4.19 | a dial of a closed link whose peer accepts and says nothing | past `T` the dial fails as a closed connection does; the request it carried is answered code 17; the log names the address and the bound | client-endpoint, "Errors reach the client through the error table" |
| C4.20 | two cores each with a link to one partition, one link closing | the other core's reads go on | role-binaries, "Each role runs one runtime per core" |
| C4.21 | a storage process restarted at its address while reads are open | the link closes, the reads are answered code 17, compute reads Members again and dials, and the next read is answered | client-endpoint, "Compute finds the partitions in Members" |
| C4.22 | a monotonic-fast read over two partitions, before its Scans | a Closed leaves on each partition's link; each link carries its own Scan after it | P §11.12, *Reads* |

## 1. The wire, `ultragraphdb_protocol_wire`

### An advertised address and a tag

Source: P §5.4. The cases run through the Join's check of section 2, the Nodes' check of each address, and the builder.

| Case | Input | Outcome |
|---|---|---|
| C1.1 | `tcp://c1.db.example:7080` | accepted |
| C1.2 | `tcp://h:1`; `tcp://h:65535` | accepted |
| C1.3 | `tcp://h:0`; `tcp://h:65536`; `tcp://h:99999` | refused |
| C1.4 | `tcp://h:07080`; `tcp://h:00` | refused: a leading zero |
| C1.5 | `tcp://h`; `tcp://h:`; `tcp://:7080` | refused: no port; no host |
| C1.6 | a DNS name of 253 bytes with port 65535, 265 bytes in all | accepted |
| C1.7 | a DNS name of 254 bytes | refused |
| C1.8 | `tcp://192.0.2.7:7080` | accepted |
| C1.9 | `tcp://[2001:db8::7]:7080`; `tcp://[::1]:7080` | accepted |
| C1.10 | `tcp://2001:db8::7:7080`; `tcp://[2001:db8::7:7080`; `tcp://[]:7080` | refused |
| C1.11 | `unix:///run/c.sock`; `TCP://h:1`; `h:1`; the empty text | refused: not `tcp://` |
| C1.12 | `tcp://h:1/`; `tcp://h:1 `; `tcp://h:1:2` | refused |
| C1.13 | bytes that are not UTF-8 | refused |
| C1.14 | the tags `a`; `query`; `high_memory`; 32 bytes of `z`; `0`; `_` | accepted |
| C1.15 | the empty tag; a tag of 33 bytes | refused |
| C1.16 | `Query`; `high-memory`; `gpu 1`; `é` | refused: a byte outside `a` to `z`, `0` to `9` and `_` |

### Nodes (27), P §5.4

Order: unused bytes 12–127 zero; `version`; sections; the string lists (2, 3) and (4, 5); section 0 a whole number of
16-byte nodes, at least one and at most 1,024; as many addresses as nodes; at most 256 tags; `self`; section 1 a whole
number of u32s, each below the tags' count; each node in order; each address; a node's tags; the references ending
with section 1; each tag.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.17 | byte 12 nonzero; byte 127 nonzero | `ReservedNotZero` | frame offsets 140 and 255 |
| C1.18 | `version` 0 with one node | `FieldValue` | `"version"` |
| C1.19 | `version` 1; `version` 2⁴² − 1 | read | |
| C1.20 | a body of five sections; of seven | section 4.1's refusal | |
| C1.21 | the addresses' `ends` of 5 bytes; an end past the text; the tags' text not UTF-8 | section 4.3's refusals, in its order | |
| C1.22 | section 0 of 15, 17 and 24 bytes | `SectionLength` | section 0, unit 16 |
| C1.23 | section 0 empty | `CountMismatch` | `"nodes"` |
| C1.24 | 1,025 nodes | `CountMismatch` | `"nodes"` |
| C1.25 | 1 node; 1,024 nodes | read | |
| C1.26 | three nodes and two addresses; three nodes and four addresses | `CountMismatch` | `"addresses"` |
| C1.27 | 257 tag strings | `CountMismatch` | `"tag strings"` |
| C1.28 | 256 tag strings, each referenced | read | |
| C1.29 | `self` equal to the nodes' count; `self` 2³² − 2 | `FieldValue` | `"self"` |
| C1.30 | `self` 0; `self` the last node's index; `self` `ABSENT` | read | |
| C1.31 | section 1 of 3, 5 and 6 bytes | `SectionLength` | section 1, unit 4 |
| C1.32 | a reference equal to the tags' count, at index 0 and at index 2 | `IndexOutOfRange` | section 1, the reference |
| C1.33 | byte 11 of a node nonzero; byte 15 nonzero | `PaddingNotZero` | the byte's body offset |
| C1.34 | node 1 with `address` 0; node 0 with `address` 1 | `FieldValue` | `"node"`, the node's index |
| C1.35 | node 0 with `tags` 1; node 1 with `tags` not where node 0's references end | `FieldValue` | `"node"`, the node's index |
| C1.36 | `tag_count` 0; `tag_count` 9 | `FieldValue` | `"node"`, the node's index |
| C1.37 | `tag_count` 1; `tag_count` 8 | read | |
| C1.38 | the last node's `tags + tag_count` one past section 1's references | `FieldValue` | `"node"`, the node's index |
| C1.39 | `state` 0; `state` 3; `state` 255 | `FieldValue` | `"node"`, the node's index |
| C1.40 | `state` 1; `state` 2 | read | |
| C1.41 | an address that is `unix://`, at node 0 and at node 2 | `FieldValue` | `"address"`, the node's index |
| C1.42 | two nodes of one address | `FieldValue` | `"address"`, index 1 |
| C1.43 | the second address below the first by its bytes | `FieldValue` | `"address"`, index 1 |
| C1.44 | `tcp://a:10` before `tcp://a:9`; `tcp://B:1` before `tcp://a:1` | read: the order is of the UTF-8 bytes, not of ports or of letters without case | |
| C1.45 | a node whose two references name one tag | `FieldValue` | `"node tags"`, the node's index |
| C1.46 | a node whose tags descend by their bytes | `FieldValue` | `"node tags"`, the node's index |
| C1.47 | a node whose references ascend by index while their tags descend by bytes | `FieldValue` `"node tags"`: the order is of the tags' bytes | the node's index |
| C1.48 | section 1 holding one reference more than the nodes' counts sum to | `CountMismatch` | `"tags"` |
| C1.49 | a tag of 33 bytes; an empty tag; a tag holding `-` | `FieldValue` | `"tag"`, the tag's index |
| C1.50 | a tag string no node references | `FieldValue` | `"tag"`, the tag's index |
| C1.51 | byte 12 nonzero and `version` 0 | `ReservedNotZero`, the earlier rule | offset 140 |
| C1.52 | section 0 empty and `self` 0 | `CountMismatch` `"nodes"`, the earlier rule | |
| C1.53 | node 0 with `state` 3 and node 1 with a padding byte nonzero | `FieldValue` `"node"`, index 0: nodes are taken in order | |
| C1.54 | a malformed address at node 0 and repeated tags at node 0 | `FieldValue` `"address"`, the earlier rule | index 0 |
| C1.55 | tags interned in another order than their first use, every tag referenced | read: the interning order is the sender's convention (P §4.7) | |
| C1.56 | 1,024 nodes, each an address of 265 bytes and eight tags out of 256 of 32 bytes | built; the frame is at most 524,288 bytes | |
| C1.57 | the golden `nodes.frame` | read, and built again to the same bytes | |

### Roster (537), P §13.26

Order: unused bytes 8–127 zero; in a request, every section empty; a reply as section 5.4 states, with `self` `ABSENT`
and the two exceptions.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.58 | byte 8 nonzero; byte 127 nonzero | `ReservedNotZero` | frame offsets 136 and 255 |
| C1.59 | `version` 0 and six empty sections | read as a request; a reply of the same bytes reads as no list yet | |
| C1.60 | `version` 0, no node and one byte in section 5; in section 1 | `CountMismatch` | `"roster"`, the sections' bytes, 0 |
| C1.61 | `version` 0 with one node | `FieldValue` | `"version"` |
| C1.62 | a reply of `version` 30, no node, `self` `ABSENT` | read | |
| C1.63 | a reply of `version` 30 and three nodes with `self` 0 | `FieldValue` | `"self"` |
| C1.64 | a reply of `version` 30, no node and one tag string | `FieldValue` | `"tag"`, index 0 |
| C1.65 | a reply of 1,024 nodes | read | |
| C1.66 | the goldens `roster.frame`, `roster_reply.frame` and `roster_reply_empty.frame` | read, and built again to the same bytes | |

### Join (512), EpochClose (515), Members (513) and the command table

Join's order, P §13.1: unused bytes 2–127 zero; `kind`; `state`; sections; section 0; section 1; section 2; the string
list (3, 4). EpochClose's, P §13.4: unused bytes 18–23 and 32–127 zero; `epoch`; `more`; `nodes`; sections, and the rest
as before.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.67 | Join: byte 2 nonzero; byte 127 nonzero | `ReservedNotZero` | frame offsets 130 and 255 |
| C1.68 | Join: `kind` 0, 1, 4 and 255 | `FieldValue` | `"kind"`: kind 1 is retired |
| C1.69 | Join: kind 2 with `state` 0 or 3; kind 3 with `state` 1 or 2 | `FieldValue` | `"state"` |
| C1.70 | Join: kind 2 with a role address in section 0; kind 2 with a partition in section 1; kind 3 with section 0 empty | `CountMismatch` `"address"`; `CountMismatch` `"partitions"`; `FieldValue` `"address"`, length 0 | |
| C1.71 | Join: kind 2 with section 2 empty, with `unix:///c.sock`, with an address of 266 bytes; kind 3 with an advertised address | `FieldValue` `"advertised"`, its length, for the first three; `CountMismatch` `"advertised"` | |
| C1.72 | Join: kind 2 with the tags `query`, `gpu` in that order; with `gpu` twice; with a tag of 33 bytes at index 1; kind 3 with one tag | `FieldValue` `"tag"`, index 1, for the first three; `CountMismatch` `"tags"` | |
| C1.73 | Join: kind 2 with no tag; with nine tags | refused: `CountMismatch` `"tags"`, the tags sent against 1 for none and 8 for nine | P §13.1 |
| C1.74 | Join: kind 2 with one tag; with eight | read | |
| C1.75 | Join: the goldens `join_compute.frame` and `join_storage.frame` | read, and built again to the same bytes | |
| C1.76 | EpochClose: byte 18 nonzero; byte 23 nonzero | `ReservedNotZero` | frame offsets 146 and 151 |
| C1.77 | EpochClose: `nodes` 2; `nodes` 255 | `FieldValue` | `"nodes"` |
| C1.78 | EpochClose: a second frame of one close whose `nodes` is not the first's | `FieldValue`, a connection error | `"nodes"` |
| C1.79 | EpochClose: `more` 2 and `nodes` 2 in one frame | `FieldValue` `"more"`, the earlier rule | |
| C1.80 | EpochClose: `nodes` 0; `nodes` 1; the golden `epoch_close_nodes.frame` | read; the golden built again to the same bytes | |
| C1.81 | Members: a reply holding a member of kind 2, at index 0 and after a storage member | `FieldValue` | `"kind"` |
| C1.82 | Members: a reply of storage members alone; of no member | read | |
| C1.83 | the command table: 27; 537 | Nodes; Roster, each giving its number back | |
| C1.84 | the command table: 523, once task 1.5 has run | no command: a frame naming it is refused as `UnknownCommand` | |

## 2. Settings of a node, `ultragraphdb_common_service_config`, `ultragraphdb_common_config_check`

Sources: config-service, "The config service validates every setting before it serves one", Compute; node-discovery, "A
compute node joins with what clients need of it"; D 6; T 2.2, T 2.3. Every refusal names the target, the instance, the
setting and the rule.

| Case | Input | Outcome |
|---|---|---|
| C2.1 | a compute instance with no `compute.advertised_address` | refused naming the setting and the instance |
| C2.2 | an advertised address of each form C1.3 to C1.5, C1.7 and C1.10 to C1.12 refuse | refused naming the setting, the value and the form it must meet |
| C2.3 | an advertised address of each form C1.1, C1.2, C1.6, C1.8 and C1.9 accept | stored |
| C2.4 | `compute.tags = []` | refused naming `compute.tags` and the instance |
| C2.5 | no `compute.tags` in the file | stored; the instance's tags read as `query` alone |
| C2.6 | one tag; eight tags | stored |
| C2.7 | nine tags | refused |
| C2.8 | `compute.tags = ["gpu", "gpu"]` | refused: a repeat |
| C2.9 | a tag of 32 bytes; of 33; the empty tag; `Query`; `high-memory` | the first stored; each other refused |
| C2.10 | `compute.tags = ["query", "high_memory"]`, not in the order of their bytes | stored; the node's Join names both in the order of their bytes |
| C2.11 | `compute_1` and `compute_2` both advertising `tcp://db.example:7080` | refused naming the address and both instances |
| C2.12 | two instances advertising one host on two ports | stored |
| C2.13 | 32 compute instances of eight tags each, 256 distinct; a 33rd instance adding a 257th | the first stored; the second refused naming the bound |
| C2.14 | an advertised address other than the instance's client address | stored: a client may reach the node at another address than it listens at |
| C2.15 | one revision whose `compute_1` lacks an address and whose `compute_2` holds nine tags | refused listing both defects |
| C2.16 | a revision changing an instance's advertised address or tags against the active revision | stored: the Fixed rule names neither setting |
| C2.17 | each profile of the repository through `ultragraphdb_config_cli` | accepted, each compute instance naming its advertised address |

## 3. The node list

### A Join at the system group, `ultragraphdb_common_system`, `ultragraphdb_cluster_metadata`

Sources: P §11.14, §13.1; node-discovery, "A compute node joins with what clients need of it", "The node list is a
function of the log"; metadata-group, "The metadata group holds the node list".

| Case | Input | Outcome |
|---|---|---|
| C3.1 | the first Join of a compute node | answered with its fields once durable; the next close holds `nodes` 1; a Roster then holds the node at that close's epoch as `version` |
| C3.2 | a Join held by an unfinished sync | no answer leaves until the sync ends |
| C3.3 | a Join equal to the entry it names, on the entry's connection | answered; the list is unchanged and no close names a change |
| C3.4 | a Join for a held address with other tags; with `state` 2; with `state` 1 after it | each changes the entry, and the next close names a change |
| C3.5 | a Join for an address another connection holds; that connection then closing | the list holds one entry for the address, the second connection's; the first's close removes nothing |
| C3.6 | a Join on another connection of the node after its joining connection closed | the entry moves there; no close names a change |
| C3.7 | two Joins of two nodes within one epoch | one close names the change; both share its epoch as version |
| C3.8 | a Join, a close, a second node's Join, a close | two versions, the first naming one node and the second two |
| C3.9 | a list of 1,023 nodes and a Join of a new address; of 1,024 and a Join of a new address; of 1,024 and a Join of a held address | accepted; refused with code 15, `detail` 1,024, the list unchanged; accepted |
| C3.10 | 256 distinct tags and a Join naming a 257th; naming held tags alone; replacing an entry's only use of one tag by a new tag | refused with code 15, `detail` 256, the list unchanged; accepted; accepted, the count staying 256 |
| C3.11 | a Join of a 1,025th node naming a 257th tag | refused with code 15, `detail` 1,024: the nodes' bound is checked first (P §13.1) |
| C3.12 | a storage process's Join | as before this stage; the list gains no entry |
| C3.13 | a connection authenticated as `storage` sending a Join of kind 2 | refused: `FieldValue` `"kind"`, by the system group's session, which knows the connection's role (P §13.1) |
| C3.14 | a Join of `state` 2 for an address the list does not hold | an entry is added, draining |

### The grace and the removal

Sources: node-discovery, "The node list is a function of the log"; system-group, "The system group serves the node
list"; T 3.3, T 3.4. Time is the group's stepped clock.

| Case | Input | Outcome |
|---|---|---|
| C3.15 | an entry's connection closed for `G` less one tick; for `G` exactly | no removal; a removal is proposed and logged, the next close names a change, and the list of that version does not hold the entry |
| C3.16 | `epoch_ms_min` 1, an entry's connection closed, twenty epochs closed, the node joining again within `G` | no removal is logged, and no close names a change |
| C3.17 | an entry's connection closed, the node dialling again and sending no Join, `G` passing | the entry is removed: a connection without a Join holds no entry |
| C3.18 | an entry's connection open for ten times `G` | never removed |
| C3.19 | a node whose joining connection closes while another is open, joining on that one at once, `G` passing | never removed |
| C3.20 | a removal logged, and the node joining afterwards | the entry is added again; the next close names a change |
| C3.21 | a removal and the same node's Join within one epoch | the list after the close equals the list before it, and the close holds `nodes` 1: a command changed the list, and the version rises (P §11.14, §13.4) |
| C3.22 | the only listed node removed | a Roster holds the close's epoch as `version`, no node and `self` `ABSENT` |
| C3.23 | a compute node's machine no longer answering, its connection not closed | the group closes the connection once bytes it sent have gone unacknowledged by the peer's kernel for `G`, by the connection's user timeout, or an EpochClose could not be handed to a full send buffer for `G` (P §13.3), and the entry leaves once `G` more has passed with no Join for it |
| C3.24 | the same with frames unacknowledged for `G` less one tick | the connection stays open |

### Closes, Roster and the restart

Sources: P §13.4, §13.26; sequencer, "A close names a change of the node list"; metadata-group, "The metadata group
holds the node list"; node-discovery, "The node list is a function of the log".

| Case | Input | Outcome |
|---|---|---|
| C3.25 | two closes with no Join, change or removal between them | the second holds `nodes` 0 |
| C3.26 | a close naming a change whose lists take two frames | both frames hold `nodes` 1 |
| C3.27 | a Roster before any close named a change, a Join being durable already | the reply holds the bytes of its request |
| C3.28 | a node joining after the close of epoch 20 and before that of 21, and a Roster between them | the reply holds the list as the close of epoch 20 left it, at its version |
| C3.29 | a Roster from a connection authenticated as `storage` | `CommandNotServed` naming Roster |
| C3.30 | a Roster request holding a byte in a section | refused with Error 1 on its stream, `CountMismatch` `"roster"` |
| C3.31 | two nodes listed at version 12, the group's host killed and restarted, both nodes dialling and joining within `G` of its start to listen | the list holds both; no close names a change for them |
| C3.32 | the same, one node never returning | its entry is removed `G` after the group began to listen, not `G` after its connection closed before the restart |
| C3.33 | an entry's connection closed for `G` less one tick when the group's host is killed, the group listening again later | the entry stays for `G` from the start to listen |
| C3.34 | a Roster at version 20 before and after a restart | both replies hold the same nodes, tags and states |
| C3.35 | two nodes held, a snapshot taken, a third joining, the group killed and restarted | the list holds three nodes at the same log position, from the snapshot and the command after it |
| C3.36 | the group killed after a Join's command was appended and before its sync ended | recovery holds the entry or not, as the log does; the node, unanswered, joins again and the list holds it once |
| C3.37 | a close naming a change appended, the host killed and restarted, a subscriber reading from before that close | it receives the close with `nodes` 1 |
| C3.38 | two compute nodes and one storage process joined, and a Members read | the reply holds the storage process's partitions alone |
| C3.39 | a storage process receiving a close with `nodes` 1 | it reads the flag and does nothing for it |

### The compute node, `ultragraphdb_common_compute`, `ultragraphdb_compute`

Sources: node-discovery, "A compute node joins with what clients need of it", "A close tells compute that the list
changed", "A node's certificate names the address it advertises"; D 5; T 0.7, T 3.7.

| Case | Input | Outcome |
|---|---|---|
| C3.40 | a node starting with `tcp://c1.db.example:7080` and the tags `query` and `high_memory` | its Join names kind 2, state 1, that address and both tags in the order of their bytes; its Subscribe and its Roster follow on the same connection |
| C3.41 | a node whose settings name no `compute.tags` | its Join names the one tag `query` |
| C3.42 | a process of four cores | one Join leaves, on the first core's connection; every core holds the list the process read |
| C3.43 | the joining connection closing while three others are open | a Join leaves at once on an open one |
| C3.44 | every connection of the node closing at once, and one opening again | the node joins on the first connection it opens again (P §11.12, *Joining*; §11.14) |
| C3.45 | the node's Join refused with code 15 | the node logs the bound and exits with code 78 (P §13.1) |
| C3.46 | the joining connection closing before the Join's answer arrived | the node joins again on another connection; the list holds one entry |
| C3.47 | a node holding version 20 receiving a close of epoch 18 flagged `nodes`; of epoch 20 flagged; of epoch 24 not flagged | no Roster is sent for any |
| C3.48 | a node holding version 20 receiving a close of epoch 24 flagged `nodes`, in two frames | one Roster is sent; the node then holds the reply's version |
| C3.49 | a third node joining a cluster of two | the next close names a change; each node reads Roster once and holds three nodes at that close's epoch |
| C3.50 | a Roster reply of the request's bytes; of a `version` and no node | the node holds no list yet; it holds a list of no node at that version |
| C3.51 | on a Roster's stream: a late frame; a frame on a stream never opened; an Error on stream 0; a reply the view refuses | checked whole and dropped; refused and the connection closed; the peer's refusal; the connection closed (P §2.5 step 3, §2.6, §13) |
| C3.52 | a node advertising `tcp://c1.db.example:7080` over TLS with a certificate naming `c2.db.example` alone; naming `*.db.example`; a node whose endpoint is `plaintext` | exit 78 naming the host and the certificate's names, no Join sent; it starts; it starts with no check |
| C3.53 | the node's remaining asks paced by the crate's stepped clock | each ask leaves at its time on that clock; no test waits on real time (T 3.7) |

### Carried from stage 5.8, `ultragraphdb_common_storage` (T 3.6)

| Case | Input | Outcome |
|---|---|---|
| C3.54 | storage's `held_versions` reading a catalog version whose stream answers an Error 7 after its first frame | the catalog read fails; the tenant's versions do not end quietly |
| C3.55 | a stream that ends with `Push::Close` | `StorageSession::answered` counts it as the end of its stream |

## 7. Several compute nodes and the clients

### The client, `ultragraphdb_client_core`

Sources: P §2.5, §5.4, §11.14; node-discovery, "A client holds the list of the highest version", "A client is started
with seeds and picks nodes by tag", "A draining node is announced before it stops"; D 7. Each case runs over
`TestServer` extended to several servers.

| Case | Input | Outcome |
|---|---|---|
| C7.1 | one seed, whose node holds no list yet, and a commit | the commit is answered on that connection; the client waits for no list |
| C7.2 | two seeds, the first refusing its connection | the client connects to the second, after the first, with backoff |
| C7.3 | two seeds, the first refusing the client's authentication | the client reports the refusal and dials no further seed |
| C7.4 | a client holding version 24 receiving a Nodes of version 20; of version 24 | each is dropped without error; the connection stays open |
| C7.5 | a client holding version 24 receiving a Nodes of version 25 | the list is replaced whole; a node of the old list the new one does not name is given nothing new |
| C7.6 | a new list still naming two nodes the client is connected to and naming a third | both connections are kept; the third is dialled after a delay drawn evenly from 0 to 1,000 ms (P §11.14) |
| C7.7 | a list of one node, `self` 0; a list of 1,024 nodes | each is held |
| C7.8 | a Nodes whose `self` is `ABSENT`, on the client's one connection, the list naming two other nodes | the client begins nothing new on that connection, whose node the list does not name, and picks a listed serving node of its tag |
| C7.9 | a client naming no tag | its tag is `query` |
| C7.10 | a list of one node tagged `gpu` and one tagged `query`, and a client of tag `query` | the `gpu` node is sent nothing |
| C7.11 | a client of tag `high_memory` holding a list in which no serving node carries it | the request is refused by the client, the error naming `high_memory` and the tags the list holds; no frame is sent |
| C7.12 | the only node carrying the client's tag named draining | refused by the client as C7.11: no serving node carries the tag |
| C7.13 | two nodes tagged `query` and one tagged `gpu`, and 100 transactions begun by a `query` client | both `query` nodes serve some, 50 each when both stay reachable, and the `gpu` node none |
| C7.14 | a Nodes naming a node draining while a transaction is open on it | the transaction runs to its end there, its reads included; the next transaction begins on another node |
| C7.15 | a one-frame request while the list names the connection's node draining | it is sent to another serving node of the tag |
| C7.16 | every node of the list refusing its connection, and a seed answering | the client connects to the seed and keeps the version it held; a Nodes of a lower version from the seed's node is dropped, one of a higher version taken |
| C7.17 | a listed node refusing its connection and accepting again later | it is skipped, tried again after the retry delay, 50 ms by default, doubling to at most 30,000 ms, each wait drawn from half the delay to the whole, and used once it answers (P §11.14) |
| C7.18 | a Nodes frame before the client's final Authenticate | refused under P §2.5 step 3; the client closes the connection |
| C7.19 | a Nodes frame on a stream other than 0 | refused under P §2.5 step 3; the client closes the connection |
| C7.20 | a Nodes frame the view refuses, by any rule of C1.17 to C1.54 | the client closes the connection (P §2.6) |
| C7.21 | a Nodes frame flagged `LAST` | refused: `FieldValue` `"flags"`, and the client closes the connection (P §5.4) |
| C7.22 | a session registered on one node, request 1 committed there, request 2 committed through a connection to another node | request 2 commits |
| C7.23 | an Execute on one node's connection of a statement prepared on another's | refused with code 10; it succeeds once the client has prepared the statement on that connection |
| C7.24 | the connection of an open transaction closing | the transaction is aborted; a commit the close cut is in doubt and resolved by its resend, on any node |
| C7.25 | a client dialling a seed's address and an advertised address over TLS | each certificate is verified against the host of the address dialled |

### The endpoint sends the list, `ultragraphdb_common_compute`

Sources: P §5.4; node-discovery, "A client receives the list once it has authenticated"; T 7.3.

| Case | Input | Outcome |
|---|---|---|
| C7.26 | a client authenticating on a node that holds three nodes at version 20 | one Nodes frame of version 20 naming three nodes, `self` the index of the node it is connected to, on stream 0 with `flags` 0 |
| C7.27 | a peer completing Hello and Welcome at `auth` 1 and not authenticating; failing its authentication | no Nodes frame |
| C7.28 | a connection of `auth` 0 | the Nodes frame follows the Welcome |
| C7.29 | a client authenticating on a node that holds no list yet, the node reading one later | no Nodes at authentication; one once the node holds a list of at least one node |
| C7.30 | a node holding a list of no node | no Nodes frame |
| C7.31 | a node holding version 20 reading version 24 while a connection is open | the connection receives one Nodes of version 24, and none again until a higher version exists |
| C7.32 | a node whose list does not hold its own address | `self` is `ABSENT` |
| C7.33 | a node of two cores, each with client connections, reading a higher version | the connections of both cores receive it |
| C7.34 | a client whose Hello names `max_frame` 524,288 and a list at every bound (C1.56) | the Nodes frame fits |

### Draining

Sources: P §11.14, *Draining*; node-discovery, "A draining node is announced before it stops"; role-binaries,
"Readiness, health and shutdown"; T 7.3.

| Case | Input | Outcome |
|---|---|---|
| C7.35 | SIGTERM at one of two nodes while a client holds connections to both | the node joins again with `state` 2; the client receives a Nodes naming it draining before the node closes its connection, and begins its next transaction on the other node |
| C7.36 | SIGTERM with a read-write transaction open that then commits | the transaction commits on the draining node; the node stops once its last read-write transaction has ended |
| C7.37 | SIGTERM with a read-write transaction open whose client sends nothing more | the node stops no later than `T` after the transaction began |
| C7.38 | SIGTERM with a read-only transaction open | it ends with its connection when the node stops |
| C7.39 | SIGTERM with no client connection | the node joins with `state` 2 and stops; the entry stays, draining, until its removal `G` after the connection closed |
| C7.40 | SIGTERM, the close naming the node draining arriving `E` later | the node sends no Nodes naming it draining before it has read that list, and does not stop before it has sent it, unless `transaction_ms` has passed since SIGTERM (P §11.14, *Draining*) |
| C7.41 | the same with a read-write transaction that ended before the close arrived, begun `T` before | the node stops once its Nodes have gone out, and no later than `transaction_ms` after SIGTERM (P §11.14, *Draining*) |
| C7.42 | SIGTERM while the system group cannot be reached | the node stops no later than `transaction_ms` after SIGTERM, its Nodes unsent (P §11.14, *Draining*) |
| C7.43 | a Begin arriving on a draining node from a client that has not yet read the Nodes | refused with code 33; the client begins on another serving node of its tag (P §11.14, *Draining*; §9.4) |
| C7.44 | a commit the stop cuts | in doubt; its resend through the session on another node answers its stored reply or commits it once |
| C7.45 | a node killed outright, no drain | its entry stays, serving, until `G` has passed; clients skip it as unreachable |
| C7.46 | a drained node started again at the same address | its Join of `state` 1 changes the entry; the next close names a change |
| C7.47 | a third node joining and the node itself turning draining within one epoch | one close names the change; each connection receives one Nodes frame of that version holding both |
| C7.48 | a DirectWrite through a session, request `r`, arriving on a draining node | refused with code 33, nothing written, the session's counter unmoved; the same request `r` commits once on another node |
| C7.49 | a Write, a Read and a Commit of a transaction open on a draining node | each is served; code 33 answers none of them |
| C7.50 | code 33 from a node, then a Nodes of the version held naming it serving; then one of a higher version naming it serving | the client gives the node no new work after the first; it does again after the second |
| C7.51 | code 33 from the only serving node of the client's tag | the client returns code 33 to its caller, and its list is as it received it |

## 8. The cluster end to end, `ultragraphdb_tests`, `ultragraphdb_tool_test`

Runs over the built binaries. Sources: T 8.1 to T 8.4; node-discovery, "A draining node is announced before it stops",
"The node list is a function of the log"; transactions, "Exactly-once commits through sessions", "Commits answer a
session token"; schema-changes, "A partition checks the catalog version a Drop names".

| Case | Run | Outcome |
|---|---|---|
| C8.1 | two compute processes and one storage process on one host, two clients each committing and reading through a different node | each client reads the other's commit once it carries the commit's token |
| C8.2 | one session used through both nodes | each request commits once |
| C8.3 | a resend on the second node while the original is on its way from the first | answered code 14 |
| C8.4 | a node killed | its entry is gone once `G` has passed and not before; its clients are served by the other node |
| C8.5 | `epoch_ms_min` 1, a node's connection to the group cut and dialled again within `G` | nothing is removed, and no close names a change |
| C8.6 | a token of a commit through one node carried to a snapshot read through the other | the read is not below the token's epoch and returns the commit |
| C8.7 | a Drop sent through a node one close behind | refused by the partition |
| C8.8 | SIGTERM at one of two nodes under load | the drained node takes no new transaction; every acknowledged commit reads back through the other |
| C8.9 | the system group's host killed and restarted with two nodes listed | both dial and join again; the list keeps both and its version does not fall |
| C8.10 | a commit's relay measured beside a merge over four partitions on one core | it waits at most one frame's merge; a longer wait goes to the user with D 3's seam (T 8.3) |
| C8.11 | a commit's latency under a mixed load at the edge of the spread of its latency without reads; outside it | within the bound; a decision of the user's (T 8.3) |
| C8.12 | the development image with two compute instances, once per architecture, amd64 under emulation | each instance reaches `/ready` |
| C8.13 | every client-facing feature before and after the stage | the counts are equal; each edited scenario is listed with the step that named the relay or the frontend (T 8.1) |

## 9. The benchmark, `ultragraphdb_tool_bench`

Sources: T 9.1 to T 9.3; D 11.

| Case | Figure | Outcome |
|---|---|---|
| C9.1 | a write or read figure at the edge of both runs' spread; outside it | within the bound; a regression to root-cause, or recorded with its cause and the user's decision |
| C9.2 | the run with two compute nodes | recorded apart; it gates nothing |
| C9.3 | the run's settings | those of `openspec/benches/benches-2026-11-15.md` brought to two roles and changed in nothing else; the record says the difference holds stage 5.8's Check round trip and this stage's removed hop together |

## Settled in the specification (2026-11-18)

The user ruled on the fifteen items that stood open here. Each is closed in
`specification/binary_protocol/PROTOCOL.md`; `notes/group-0-protocol.md` records the edits. The item numbers are those
the cases above cite.

1. **The order a merge returns.** P §9.1 holds, as it stood: rows come partition by partition, within a partition in
   its families' order, and "no order holds by term or by key". "In key order" in T 5.5 and D 11 means each
   partition's own order; the coordinator corrects those two sentences.
2. **What a resume key's `pattern` and `scope` hash.** P §11.8 states both without ComputeRead. `pattern` hashes the
   bound pattern, the term table's records and its text, the bytes hashed before the stage; for an Execute the table
   is the statement's, then the parameters' terms in parameter order, a triple term's parts before it, each term once.
   `scope` hashes the transaction's identity, or is the version a read-only transaction reads at.
3. **The form of a host in an advertised address.** P §5.4: one encoding. A DNS name is 1 to 253 bytes of `a` to
   `z`, `0` to `9`, `-` and `.`, and upper case is refused. A host of digits and `.` alone is an IPv4 literal, four
   numbers of 0 to 255 without a leading zero. An IPv6 literal takes RFC 5952's form. Cases to add to group 1:
   `tcp://C1:1` refused; `tcp://c_1:1` refused; `tcp://192.0.2.07:1` and `tcp://256.0.0.1:1` refused;
   `tcp://[2001:DB8::7]:1` and `tcp://[2001:db8:0:0:0:0:0:7]:1` refused.
4. **A Join once every connection of a node has closed.** P §11.14 and §11.12, *Joining*: the node joins on the first
   connection it opens again.
5. **A compute node whose Join is refused with code 15.** P §13.1: it logs the bound and exits with code 78.
6. **A drain whose announcement is late or cannot be made.** P §11.14, *Draining*: the node stops no later than
   `transaction_ms` after it began to drain, whether or not its Nodes went out, and no earlier than its Nodes when
   they go out within that time. node-discovery, "A draining node is announced before it stops", says the same.
7. **The bounds of a client's delays.** P §11.14: a newly listed node is dialled after a delay drawn evenly from 0 to
   1,000 ms; the backoff starts at the client's retry delay, 50 ms by default, doubles to at most 30,000 ms, and each
   wait is drawn evenly from half the delay to the whole; the seeds wait so between two passes. The 1,000 ms and the
   30,000 ms are figures proposed with this ruling, since neither the document nor `ultragraphdb_client_core` held
   them; the 50 ms is the client's `retry_delay`.
8. **"Unacknowledged by the transport".** P §13.3: for twice `epoch_ms_max`, either an EpochClose the group could not
   hand to the connection, its send buffer full, on either carrier, or on TCP bytes the peer's kernel has not
   acknowledged, by the connection's user timeout.
9. **A wildcard host in an advertised address.** P §5.4: `0.0.0.0` and `[::]` are refused. Cases to add to group 1:
   `tcp://0.0.0.0:7080` and `tcp://[::]:7080` refused.
10. **A compute node's Join with no tag or nine.** P §13.1: `CountMismatch` `"tags"`, the tags sent against 1 for
    none and 8 for more.
11. **A Join past both bounds.** P §13.1: the nodes' bound is checked first, so `detail` is 1,024.
12. **A Join whose kind is not its connection's role.** P §13.1: refused with `FieldValue` `"kind"`, by the system
    group's session.
13. **`nodes` for a list that changed and changed back within one epoch.** P §11.14 and §13.4: `nodes` is 1 when any
    command that added, changed or removed an entry was applied since the previous close, and the version rises.
15. **A Nodes frame whose `flags` are not 0.** P §5.4: refused with `FieldValue` `"flags"`, and the client closes the
    connection.
14. **A Begin arriving at a draining node.** P §9.4 and §11.14, *Draining*: a new code 33 (Draining). From the moment
    it begins to drain a node refuses a Begin, a DirectWrite, a ConditionalWrite, an Ingest and a Drop with it, writing
    nothing; the client sends the request to another serving node of its tag and gives the refusing node no new work
    until a list of a higher version names it serving. The client's list itself is never edited (P §11.14, "never
    edits its list itself"), so the mark stands beside it. Cases C7.43 and C7.48 to C7.51; law L7.15.
