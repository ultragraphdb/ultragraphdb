# Stage 5.9: the laws of every group

Task 0.4. A law holds for all inputs of a stated kind (`openspec/protocols/tdd-protocol.md`, "Phase 2 states every law
as a property test"). Each law below is one Hegel property test of its group's phase 2: written against the
unimplemented surface, observed failing, under `tests/property/` in the crate's `property` target, tagged
`property-test`. Each computes what it expects from the drawn inputs by a rule independent of the code under test:
bytes laid out by hand from the section's table, a model written from the specification, or a surface this stage leaves
unchanged.

The kinds are the protocol's: round trip, refusal, order, inverse, bound, state machine; and two this stage adds, model
equivalence (two paths give one answer) and order independence (a permutation of the input changes nothing). Generators
draw sizes and counts inside the stated limits and well below them, so each target runs in seconds. A property test
replaces no scenario, no golden frame and no case of `notes/corner-cases.md`.

Sources are cited as `P §n` for `specification/binary_protocol/PROTOCOL.md`, as `capability, "requirement"` for the
change's `specs/`, as `D n` for `design.md` and as `T n` for `tasks.md`. `PROTOCOL.md` is cited by section number, with
the "From stage 5.9" paragraphs read as part of the sections they amend (T 0.2); no section is renumbered.

The groups stand in the order they land (D 12).

| Group | Laws | Target |
|---|---|---|
| 5, the endpoint executes reads | 14 | `ultragraphdb_common_compute`, `property` |
| 6, the switch | 5 | `ultragraphdb_common_config_check`, `ultragraphdb_common_system`, `ultragraphdb_common_role`, each `property` |
| 4, one connection per core and partition | 4 | `ultragraphdb_common_compute`, `property` |
| 1, the wire | 18 | `ultragraphdb_protocol_wire`, `property` |
| 2, settings of a node | 5 | `ultragraphdb_common_service_config`, `ultragraphdb_common_config_check`, each `property` |
| 3, the node list | 19 | `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_common_system`, `ultragraphdb_common_compute`, each `property` |
| 7, several compute nodes and the clients | 15 | `ultragraphdb_client_core`, `ultragraphdb_common_compute`, each `property` |
| 8, the cluster end to end | none | |
| 9, the benchmark | none | |

`ultragraphdb_common_compute`, `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` and
`ultragraphdb_client_core` hold no `property` target today. The first group that writes a law in each creates it, with
its tests, and the merge of T 5.2 carries `ultragraphdb_common_frontend`'s `property` target into
`ultragraphdb_common_compute` with its tests unchanged.

## 5. The endpoint executes reads, `ultragraphdb_common_compute`

The surface is the executor's message seam (D 3): the session hands the executor a read as a message and takes its
frames as messages. Every law drives that seam with a played partition per drawn partition and a stepped clock. The
model of a read's answer is written from P §9.1: the rows of the partitions asked, partition by partition in ascending
order of partition, within a partition in the order its Entries give them.

T 5.5 names the first, the fourth and the eighth. Its words "in key order" are read as P §9.1's order, which is the
order of the partitions' entries concatenated; `notes/corner-cases.md`, *Settled in the specification*, item 1.

### The merge and its frames

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L5.1 | Order | 1 to 4 partitions, each with 0 to 64 entries in its own order; per partition the boundaries that cut its entries into Entries frames; the client's `max_frame` from the floor up; credit that never runs out | the rows of the read's Result frames, concatenated, are the model's rows in the model's order, each once, whatever the boundaries of the Entries frames and of the Result frames | P §9.1, §11.12, *Reads*; T 5.5; D 11 |
| L5.2 | Bound | the read of L5.1 | every Result frame is at most the client's `max_frame`; the last alone is flagged `LAST`; section 6 is empty in every other; the stream holds at least one frame, one of `rows` 0 when nothing matched | P §9.1; quad-reads, "A read's batches fit the client's frame" |
| L5.3 | Refusal | the read of L5.1 with one row's terms drawn from below to above what an otherwise empty batch of the client's `max_frame` holds | the read is refused with code 6, `detail` the client's `max_frame`, exactly when a row does not fit an otherwise empty batch, and no row after it is sent | quad-reads, "A read's batches fit the client's frame" |
| L5.4 | Inverse | the read of L5.1 under a drawn `limit`; a row `r` drawn from the rows returned; the same read sent again with the resume key of a page ending at `r` | the resumed read returns exactly the model's rows after `r`, none repeated and none skipped; pages of a drawn size read one after another by their keys concatenate to the unpaged answer | P §11.8; T 5.5 |
| L5.5 | Round trip | a resume key's fields: a pattern hash, a scope, a partition of at least 1, a last row's five terms and an ordering value | the key decodes to the fields drawn; with one drawn byte changed it is refused with Error 1 naming the resume key; a key built for another pattern or scope is refused the same way | P §11.8 |
| L5.6 | Inverse | the read of L5.4, the first page read through one executor and the resumed page through another, built apart over the same played partitions | the second executor accepts the first's key and returns the rows after `r`: a key is a function of the request and the row, and of no state an executor holds | P §11.8 ("after a lost connection"); node-discovery, "A client is started with seeds and picks nodes by tag" (a session serves through any node) |

### Credit, Cancel and the step

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L5.7 | Bound | the read of L5.1; an `initial_credit` of 1 to 4; Credit frames of drawn `frames`, arriving between any two steps and in the turn of a step | at every point the Result frames sent on the stream are at most `initial_credit` plus the Credits read, the sum saturating at 2³² − 1; a frame that credit allows is sent in the turn the Credit is read; with enough credit every row arrives | P §11.3; quad-reads, "A read's batches fit the client's frame"; D 4 |
| L5.8 | State machine against a model | the read of L5.1; a Cancel arriving before the first Scan, between two frames, in the turn of a step, after `LAST`; Credit frames after it | no Result frame of the stream reaches the client after the turn the Cancel is read; each of the read's Scans still open is cancelled in that turn, each once; a Credit afterwards changes nothing; the read's transaction goes on | P §11.4; quad-reads, "A read's batches fit the client's frame" ("A Cancel stops a read without a relay"); T 5.4 |
| L5.9 | Bound | 1 to 4 reads under way on one core, each with frames ready, and commits arriving for relay at drawn points; any interleaving the core's scheduler may take | a step consumes at most one Entries or Neighbours frame, or builds at most one Result frame; no commit waits for more than one step before it is relayed | quad-reads, "A read's step is bounded by one frame"; D 3; T 5.5 |
| L5.10 | Order | the reads of L5.9, each ready at every step | between two steps of one read every other ready task of the core has run a step | quad-reads, "A read's step is bounded by one frame" ("A read's step stays within one frame") |

### A transaction's order, the routing and the seam

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L5.11 | State machine against a model | one read-write transaction: a sequence of Writes, Reads, a read's Credit and Cancel, and a Commit or an Abort; the partition's Held, Entries and Errors arriving at drawn points | a read's first Scan leaves only once every write the client sent before it is answered; a write leaves only once every read sent before it is answered, by its `LAST`, an Error or its Cancel; reads with no write between them run beside one another; a Commit cancels the open reads, drops the held ones unsent, sends the held writes in the order sent and then the Commit; an Abort cancels the reads and drops the held writes; no frame of a cancelled read reaches the client | quad-reads, "A read through compute is a read of its transaction", "Compute runs each read on one core"; P §11.12, *Reads* |
| L5.12 | Bound | the transaction of L5.11 with Writes of drawn sizes behind an open read; `held_bytes` drawn small | the bytes held behind the connection's reads never pass `held_bytes`; the Write that would take them past it is refused with code 15, `detail` the bound, and ends the transaction | client-endpoint, "Compute bounds what a connection's reads hold back"; P §11.9 |
| L5.13 | State machine against a model | a pattern with each position bound or unbound; a placement map over 1 to 4 partitions; a level; a token's epoch; the partitions' latest closed epochs | the partitions asked are those P §11.12's table names for the bound positions, and no other; a monotonic-fast read answers at the lowest closed epoch among them and never below its floor; a transaction's answered epochs never fall; a latest-state read reaching a second partition is refused with code 25 | P §11.12, *Reads*; client-endpoint, "Requests are routed to the partitions that serve them"; quad-reads, "A read's transaction names its level" |
| L5.14 | Model equivalence | the read of L5.1 with a drawn `initial_credit` and Credits, run once through the session's seam in process and once as a ComputeRead through `ComputeSession`, over the same played partitions | both give the same Result frames, byte for byte, in the same order | D 2 ("one executor serves the endpoint in process and … the compute binary's role session"); T 5.4, T 5.6 |

How L5.7 and L5.8 read "in the turn" (decided 2026-11-18, `notes/group-5-design.md`, question 4). L5.7: a read builds its
next Result frame only once the frame before has left and the stream holds credit, so no frame waits on credit in the
connection's outbound queue; a Credit wakes the read in the turn it is read, and the frame it allows leaves in the read's
next step. L5.8: from the turn the Cancel is read no Result frame of the stream reaches the client, and the read takes no
further step; its Scans' Cancels are queued when the core drops the read's task at the task's next slot of the core's
pass, before any other step of that read. L5.9 is stated over the core's scheduler, from the moment a commit's task is
ready; task 8.3 measures the wait from the commit's bytes arriving (question 3).

L5.14 holds while both callers exist. Task 6.4 removes `ComputeSession`'s role session and this test with it, after the
user's approval; `notes/group-6.md` records the removal.

## 6. The switch

T 6.7 says "this group's binaries and deployment hold no law of their own". That holds for the binaries, the
supervisor, the images, the chart and `scripts/cluster_local.sh`: they order starts and write files, which scenarios
cover, and `ultragraphdb_supervisor` and the chart gain no `property` target. It does not hold for three library crates
the switch changes, whose rules hold for all inputs of a kind:

| Law | Kind | Crate | Inputs drawn | Assertion | Source |
|---|---|---|---|---|---|
| L6.1 | Refusal | `ultragraphdb_common_config_check` | a target of 0 to 3 compute and 0 to 3 storage instances, each otherwise valid | the Roles check accepts exactly the targets holding at least one compute and at least one storage instance, and names the target and the role that is absent | config-service, "The config service validates every setting before it serves one", Roles; T 6.3 |
| L6.2 | Refusal | `ultragraphdb_common_config_check` | an accepted target with one retired name added to a drawn file: a `[frontend]` table, `compute.role_address`, `cluster.frontend_address`, or an instance whose name begins `frontend` | it is refused naming the table or the setting as unknown, or the instance by the Names rule, and no other defect is listed | config-service, the same requirement ("The retired role's settings are refused by name"), Names; T 6.3 |
| L6.3 | Refusal | `ultragraphdb_common_config_check` | an accepted target; a drawn instance file given one table of the other role: `[storage]` in a compute instance's file, `[compute]` in a storage instance's | it is refused naming the table and the instance; a `[metadata]` or `[sequencer]` table outside the hosting instance is the System check's, as before | T 0.1 ("a role's file holding another role's table, refused by name"); T 6.3 |
| L6.4 | Inverse | `ultragraphdb_common_system` | a role, `compute` or `storage`; a command number, any u16 | the system group serves the command to the role exactly when P §11.12a's table names the role in the command's row; every other number is refused as `CommandNotServed` naming it, or as `UnknownCommand` when section 3 holds no such number, 523 among them | P §11.12a; system-group, "The system group serves the books' role commands at its role address"; T 6.3 |
| L6.5 | Refusal | `ultragraphdb_common_role` | a SCRAM username of 1 to 16 bytes, drawn from the two role names, `frontend`, and strings one edit away from each | a role connection authenticates under the name exactly when it is `compute` or `storage`; every other name is refused as a wrong secret is | P §11.12a ("the SCRAM usernames of role connections are `compute` and `storage`"); wire-protocol, "The node list's frames are bodies of the protocol" |

The Compute check of the same requirement is group 2's (L2.1). T 6.6's idle limit is one boundary on a clock, a corner
case (C6.20).

## 4. One connection per core and partition, `ultragraphdb_common_compute`

The surface is `RoleLink` carrying a core's Scans, Frontier requests and Closed beside its Bind streams (D 2). The
played partition of T 4.2 answers; its tap grants or withholds what each law needs.

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L4.1 | State machine against a model | on one `RoleLink`: Binds with their Writes, one-frame writes, Scans, Frontier requests and Closed opened in a drawn order; the partition's replies, each one its request's "Answered by" names or a drawn other; a request that cannot be built or sent; a failed write; a close by the peer | a reply is taken exactly when a request recorded on its stream is answered by its command; any other frame on an open stream closes the link; a stream whose request could not be sent is registered nowhere afterwards; once the link is closed, by a refusal, a failed write or the peer, each open stream is answered code 17 exactly once, no Scan of such a read is sent again, and the next request dials again | P §2.5 step 3, §2.6; client-endpoint, "Errors reach the client through the error table"; D 2; T 4.2; `notes/feasibility.md` §4, cases 1 and 2 |
| L4.2 | Bound | per Scan stream, the credit compute granted and the Entries frames the played partition sends, from below the credit to past it | the link stays open while each stream's frames are within its credit, and closes at the first frame past it, as `PastCredit` naming the stream, sending no Error; every other read the link carried is then answered code 17 | P §11.3; wire-protocol, "A frame past its stream's credit is refused"; `notes/feasibility.md` §4, case 3 |
| L4.3 | Refusal | a frame on a stream whose reader is gone, numbered below the next stream the link will open: valid, or with one rule of P §2.5 steps 4 to 15 broken | the valid frame is dropped and the link stays open; the broken one is refused by its rule and closes the link; a frame on a stream at or above that number closes the link whatever it holds | P §2.5 step 3, §11.2; T 0.7; `notes/feasibility.md` §4, case 4 |
| L4.4 | Order independence | 2 to 4 played partitions, bound to ports in a drawn order and listed in the read's subject in another | each request reaches the partition its subject names, and the read's rows are those of L5.1's model, whatever order the doubles bound their ports in | P §9.1 (partitions ascend by number, not by address); `notes/feasibility.md` §4, case 5 |

L5.1 to L5.13 hold unchanged over `RoleLink`: group 4 runs them with the connection changed beneath them and writes
none of them again. L5.11's order is then kept by one task queue (D 2), which the law does not name.

## 1. The wire, `ultragraphdb_protocol_wire`

A drawn list is 0 to 16 nodes and 1 to 12 distinct tags, each node an advertised address of a drawn host form, 1 to 8
tags and a state. A bound's own test draws at the bound.

### Round trips

A body built from the drawn fields is read back by its view to the same fields, and builds again to the same bytes. The
expected bytes are laid out by hand from the section's tables.

| Law | Body | Inputs drawn | Source |
|---|---|---|---|
| L1.1 | Nodes (27) | a list of at least one node; `version` an epoch of at least 1; `self` an index below the nodes' count or `ABSENT` | P §5.4; T 1.3 |
| L1.2 | Roster (537), request and reply | the request: `version` 0 and six empty sections; a reply: a list of 0 or more nodes at a `version` of at least 1, `self` `ABSENT`; the reply before any change: the request's bytes | P §13.26; T 1.3 |
| L1.3 | Join (512), compute | `state` 1 or 2; an advertised address; 1 to 8 tags ascending by their bytes; sections 0 and 1 empty | P §13.1 |
| L1.4 | Join (512), storage | a role address on either carrier; 1 to 8 partitions ascending; `state` 0; sections 2 to 4 empty | P §13.1 |
| L1.5 | EpochClose (515) with `nodes` | the fields it holds today and `nodes` 0 or 1; a close of 1 to 3 frames, `nodes` the same in each | P §13.4 |
| L1.6 | Members (513), a reply | 0 to 8 storage members, partitions ascending, and no member of kind 2 | P §13.2 |

### Bounds and grammars

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L1.7 | Bound | a list at its bounds in a drawn mix: up to 1,024 nodes, addresses up to 265 bytes, up to 8 tags a node out of up to 256 tags of up to 32 bytes | the Nodes frame and the Roster reply are each at most 524,288 bytes; the size equals the sum laid out by hand: the header, the section table, 16 bytes a node, 4 a tag reference, the two string lists | P §5.4 ("every list fits one frame at the floor of `max_frame`"); wire-protocol, "The node list's frames are bodies of the protocol"; T 1.3 |
| L1.8 | Refusal | a text built from a drawn scheme, host and port: the scheme `tcp://` or another; the host a DNS name, an IPv4 literal, an IPv6 literal in brackets, or one of those broken: an upper-case letter, a character outside the name's, a number with a leading zero or above 255, an IPv6 literal not in RFC 5952's form, a wildcard; the port 0 to 99,999 with and without a leading zero, or absent | the advertised address is accepted exactly when the scheme is `tcp://`, the host is of a stated form and the port is 1 to 65,535 in decimal without a leading zero; no accepted address passes 265 bytes | P §5.4; T 1.3 |
| L1.9 | Refusal | a text of 0 to 40 bytes drawn from `a`–`z`, `0`–`9`, `_` and other bytes | a tag is accepted exactly when it is 1 to 32 bytes of `a` to `z`, `0` to `9` and `_` | P §5.4, §13.1 |
| L1.10 | Inverse | a command number, any u16 | the command table reads 27 and 537 as Nodes and Roster and gives each its number back; it reads 523 as no command | P §3; wire-protocol, the same requirement ("the number is refused as an unknown command and never used again"); T 1.2 |

### Refusals

A valid body is drawn, one rule of its "Order" line is drawn and broken by an edit to the bytes laid out by hand, and
the view refuses it with that rule's refusal, its field and its index, and with no other. The rules of each body are
those `notes/corner-cases.md` lists for it.

| Law | Body | Rule drawn from | Source |
|---|---|---|---|
| L1.11 | Nodes | an unused byte of 12 to 127; `version` 0; a section 0 not a multiple of 16; no node; more than 1,024 nodes; an address count other than the nodes'; more than 256 tags; `self` at or past the nodes' count and not `ABSENT`; a section 1 not a multiple of 4; a reference at or past the tags' count; a node's padding byte; a node's `address` not its index; its `tags` not where the node before's end; `tag_count` 0 or 9; `tags + tag_count` past section 1; `state` 0 or 3; an address malformed, equal to or below the one before; a node's tags descending or repeated; references left over after the last node's; a tag malformed; a tag no node references. Each names the index its rule states | P §5.4; T 1.3 |
| L1.12 | Roster | an unused byte of 8 to 127; a request holding a byte in any section; a reply whose `self` is not `ABSENT`; a reply broken by a drawn rule of L1.11 | P §13.26 |
| L1.13 | Join | an unused byte of 2 to 127; `kind` 0, 1 or 4; `state` 0 or 3 for kind 2 and 1 for kind 3; a role address in a compute node's section 0; an advertised address malformed or absent for kind 2, present for kind 3; no tag, nine tags, a tag malformed, repeated or below the one before for kind 2; a tag for kind 3 | P §13.1; wire-protocol, the same requirement ("A compute Join names no role address") |
| L1.14 | EpochClose | `nodes` 2 to 255; an unused byte of 18 to 23; across the frames of one close, a frame whose `nodes` is not its predecessor's | P §13.4 |
| L1.15 | Members | a reply holding a member of kind 2 at a drawn index | P §13.2; wire-protocol, the same requirement ("Members holds no compute node") |

### The order, the builders and the tags' interning

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L1.16 | Order | a valid Nodes, Roster reply or compute Join; two distinct rules of its "Order" line | with both broken, the refusal is that of the rule earlier in the line | wire-protocol, the same requirement ("Every decoder SHALL enforce its body's order of checks"); T 1.4 |
| L1.17 | Refusal | the fields of a Nodes, a Roster or a Join, drawn valid or with one rule broken | the builder refuses exactly when the view refuses the same fields laid out by hand, and with the same refusal | wire-protocol, the same requirement ("a builder SHALL refuse to build what its decoder refuses") |
| L1.18 | Order independence | a valid Nodes frame; a permutation of its tag strings, each reference rewritten to follow its tag | the view reads the same nodes, each with the same tags and state: a receiver enforces that every tag is referenced and holds the interning order to no rule | P §4.7 (the row of Nodes and Roster) |

## 2. Settings of a node, `ultragraphdb_common_service_config`, `ultragraphdb_common_config_check`

| Law | Kind | Crate | Inputs drawn | Assertion | Source |
|---|---|---|---|---|---|
| L2.1 | Refusal | `ultragraphdb_common_config_check` | a target of 1 to 4 compute instances and one storage instance; per compute instance an advertised address present or absent, well formed or not, and a tag list absent, empty, of 1 to 9 tags, with or without a repeat or a malformed tag; two instances sharing an address or not; the distinct tags of the target from below 256 to above | the Compute check refuses exactly the targets with a compute instance lacking an address or holding a malformed one, a tag list empty, longer than eight, repeated or malformed, two instances advertising one address, or more than 256 distinct tags; each refusal names the target, the instance, the setting and the rule | config-service, "The config service validates every setting before it serves one", Compute; T 2.3 |
| L2.2 | Bound | `ultragraphdb_common_config_check` | an accepted target with 1 to 4 independent Compute defects planted in drawn instances | the refusal lists every planted defect and no other | config-service, the same requirement ("SHALL list every defect found, not the first") |
| L2.3 | Round trip | `ultragraphdb_common_service_config` | a compute instance's file with a drawn `compute.advertised_address` and 1 to 8 drawn tags | `ComputeTable` reads both back as drawn, the tags in the order written | D 6; T 2.2 |
| L2.4 | Inverse | `ultragraphdb_common_service_config` | a compute instance's file naming no `compute.tags` | its tags read as the one tag `query` | node-discovery, "A compute node joins with what clients need of it"; D 6 |
| L2.5 | Model equivalence | `ultragraphdb_common_config_check` | an advertised address and a tag list, drawn valid or with one rule broken | the Compute check accepts the pair exactly when the wire's Join builder builds a compute node's Join from it, the tags sorted by their bytes: nothing the config service serves is refused at a node's Join for its form | config-service, the same requirement ("so nothing it serves can be refused by a later check"); P §13.1 |

L2.5 is written in group 2 against the Join builder group 1 lands beside it. The two run in one wave (D 12), so the
test is observed failing against both unimplemented surfaces and passes when both are built.

## 3. The node list

### The list, one state machine

One Hegel state machine against a model written from P §11.14, §13.1, §13.4 and §13.26. The model holds: the entries,
each an advertised address with its tags, its state and the connection of its latest Join; per connection whether it
is open and when it closed; the list as the latest close that named a change left it, with that close's epoch; whether
the list changed since the previous close; the time; the time the group began to listen. The machine runs
`SystemSession` over the metadata state machine and the sequencer on an `OrderedLog` double, with a stepped clock and a
drawn `epoch_ms_max`. A run starts from a `MetadataSnapshot` holding a list of a drawn size, a few nodes in most runs
and a list within eight nodes or eight tags of a bound in some, so the bounds are met without 1,024 Joins; each bound is
also pinned by a corner case (C3.9, C3.10).

| Command | Inputs drawn |
|---|---|
| Join | an open connection; an address, new or one the list holds; 1 to 3 tags; `state` 1 or 2 |
| Close connection | an open connection |
| Open connection | a new connection |
| Pass | a span of time around twice `epoch_ms_max`: the exact boundary and one tick before it among the values |
| Close epoch | none: the sequencer closes its open epoch |
| Roster | an open connection |
| Restart | none: the group stops and begins to listen again, every connection closed |
| Recover | a prefix of the log: a second state machine is built from it |
| Sync | holds or releases the metadata log's sync |

After every command the machine's answer equals the model's, and these hold:

| Law | Invariant | Source |
|---|---|---|
| L3.1 | One version names one list: two Roster replies of equal `version`, at any two points of the run and across a Restart, hold the same nodes, tags and states | node-discovery, "The node list is a function of the log"; P §11.14; T 3.4 |
| L3.2 | A Roster reply's `version` never falls, and is the epoch of the latest close that named a change | P §13.26; T 3.4 |
| L3.3 | An entry belongs to the connection of the latest Join for its address: a Join on another connection moves it, and the first connection's close then removes nothing | P §11.14, §13.1; T 3.4 |
| L3.4 | An entry leaves exactly once its connection has been closed for twice `epoch_ms_max`, counted from the later of the close and the group's start to listen; never before, never while the connection is open, and whatever the number of closes of epochs in between | node-discovery, the same requirement; system-group, "The system group serves the node list"; T 3.4 |
| L3.5 | A node whose joining connection closes while another of its connections is open, and which joins on that one, is never removed, and no close names a change for it | node-discovery, "A compute node joins with what clients need of it"; T 3.4 |
| L3.6 | The list never passes the nodes' bound or the tags' bound; a Join that would take it past either is refused with code 15, `detail` the bound, and changes nothing | P §13.1; metadata-group, "The metadata group holds the node list"; T 3.4 |
| L3.7 | Recovery from any prefix of the log gives the list the model holds at that prefix, from a snapshot and the commands after it alike | metadata-group, the same requirement; T 3.4 |
| L3.8 | A Roster reply holds the list as the latest close that named a change left it: a Join after that close shows in no reply before the next close; before any close named a change the reply is the request's bytes | P §13.26; metadata-group, the same requirement |
| L3.9 | A close holds `nodes` 1 exactly when an entry was added, changed in its tags or state, or removed since the previous close; a Join that changes neither, and one that only moves an entry to another connection, names none | sequencer, "A close names a change of the node list"; node-discovery, "A compute node joins with what clients need of it" ("no close names a change of the list") |
| L3.10 | A Join is answered only once its command is durable: while the sync is held no answer leaves, and the answer holds the Join's fields | P §13.1; system-group, "The system group serves the node list" |
| L3.11 | Every Roster reply the group builds is one the wire's view accepts: nodes ascending by their addresses' bytes, one node per address, each node's tags ascending, every tag referenced | P §5.4, §13.26 |
| L3.12 | Members holds storage members alone, whatever compute nodes have joined | P §13.2; client-endpoint, "Compute finds the partitions in Members" |

### The books

| Law | Kind | Crate | Inputs drawn | Assertion | Source |
|---|---|---|---|---|---|
| L3.13 | Round trip | `ultragraphdb_cluster_metadata` | the list's commands, a Join's and a removal's, with drawn fields; a `MetadataSnapshot` holding a drawn list | each `MetadataCommand` and the snapshot encode and decode to the same value | metadata-group, "The metadata group holds the node list" |
| L3.14 | Order independence | `ultragraphdb_cluster_metadata` | a log of the list's commands; two state machines applying it, one in one run and one from a snapshot at a drawn position, under two clocks that disagree | both hold the same list after every command: applying reads no clock, since a removal is a logged command | node-discovery, "The node list is a function of the log"; P §11.14 ("the removal is a logged command all the same") |
| L3.15 | Inverse | `ultragraphdb_cluster_metadata` | a list and a Join equal to an entry it holds, on the entry's connection | applying it twice equals applying it once, and the list is unchanged | P §13.1 ("adds its entry or changes it") |
| L3.16 | Round trip | `ultragraphdb_cluster_sequencer` | a close record with a drawn flag beside the fields it holds today; a restart between append and delivery | the `SequencerRecord` encodes and decodes to the same value, and a subscriber reading from before the close receives it with the flag as appended, the same in every frame of the close | sequencer, "A close names a change of the node list" |

### The compute node

| Law | Kind | Crate | Inputs drawn | Assertion | Source |
|---|---|---|---|---|---|
| L3.17 | State machine against a model | `ultragraphdb_common_compute` | closes in any order of epochs, replays among them, each with `nodes` 0 or 1; Roster replies of drawn versions, the request's bytes and a list of no node among them | the node sends a Roster at its start and for a close flagged `nodes` whose epoch is above the version it holds, and for no other; the list it holds is that of the highest version it has read and never goes back; a reply of the request's bytes leaves it holding no list | node-discovery, "A close tells compute that the list changed"; P §13.26 |
| L3.18 | State machine against a model | `ultragraphdb_common_compute` | a process of 1 to 4 cores, each with a connection to the group; connections closing and opening again in any order; the group's answers | the process sends its first Join once, on its first core's connection; whenever the connection of its latest Join closes while another is open, a Join leaves on an open one in that turn; when none is open, a Join leaves on the first that opens; at no point do two connections carry an unanswered Join | P §11.14; D 5; T 3.7; P §11.12, *Joining* |
| L3.19 | Bound | `ultragraphdb_common_compute` | the cores of L3.18 and the lists of L3.17 | every core of a process holds the list its process read: after each Roster reply, no core answers from an older version than another core of the same process | D 5 ("every core holds the list its process read"); T 3.7 |

## 7. Several compute nodes and the clients

### The client, one state machine, `ultragraphdb_client_core`

One Hegel state machine in `ultragraphdb_client_core` against a model written from P §11.14 and §5.4, over `TestServer`
extended to several servers (T 7.3). The model holds: the seeds in their configured order; the client's tag; the list
held and its version; per address whether the server accepts a connection, refuses one or refuses its authentication;
per connection its open transactions; the time.

| Command | Inputs drawn |
|---|---|
| Nodes | a connection; a list of 1 to 4 nodes with tags and states; a version at, below or above the one held; `self` |
| Refuse, restore | an address, a seed's or a listed node's |
| Begin, one-frame request | none |
| Request in a transaction | an open transaction |
| End | an open transaction |
| Pass | a span of time around the backoff the model holds |

After every command these hold:

| Law | Invariant | Source |
|---|---|---|
| L7.1 | The list held is the one of the highest version received on any connection, whole: it equals one received frame's list and never mixes two; a frame at or below the version held changes nothing and closes nothing | node-discovery, "A client holds the list of the highest version"; P §5.4; T 7.4 |
| L7.2 | The version held never goes back, a return to the seeds included | P §11.14 ("keeping the version it holds"); T 7.4 |
| L7.3 | No transaction begins and no one-frame request leaves on a node the list names draining or names no longer | node-discovery, "A draining node is announced before it stops"; T 7.4 |
| L7.4 | Once the client holds a list, every node chosen carries the client's tag; before any list arrives the client uses the connection it has and checks no tag | node-discovery, "A client holds the list of the highest version", "A client is started with seeds and picks nodes by tag"; T 7.4 |
| L7.5 | A client with a reachable serving node of its tag is never refused by itself; a request is refused by the client, naming its tag and the tags the list holds and sending no frame, exactly when no serving node of the list carries the tag | node-discovery, "A client is started with seeds and picks nodes by tag"; T 7.4 |
| L7.6 | A seed is dialled only while no listed node can be reached, or while no list is held; seeds are tried in the configured order | node-discovery, the same requirement; P §11.14, *Seeds*; T 7.4 |
| L7.7 | A refused authentication ends the attempt: no further seed is dialled for it, and the refusal is reported | node-discovery, the same requirement ("A wrong password ends the attempt for every seed") |
| L7.8 | The reachable serving nodes of the tag are taken in turn: over any run of begins and one-frame requests between two changes of that set, the counts two such nodes served differ by at most 1 | D 7 ("taken in turn"); node-discovery, the same requirement ("A client spreads over the nodes of its tag") |
| L7.9 | Every request of a transaction leaves on the connection its Begin left on, whatever lists arrive meanwhile; a transaction open on a node named draining runs to its end there | node-discovery, the same requirement, and "A draining node is announced before it stops" |
| L7.10 | The client keeps each connection a new list still names, dials a newly listed node within 1,000 ms, and dials a listed node it could not reach no earlier than half its backoff's delay and no later than the whole of it, the delay starting at the retry delay and doubling to at most 30,000 ms | node-discovery, the same requirement; P §11.14 |
| L7.11 | A session's requests leave through any connection: a Commit of a session registered on one node is sent on another without a second Register | node-discovery, the same requirement ("A session serves through a second node") |

### The endpoint, `ultragraphdb_common_compute`

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L7.12 | State machine against a model | one endpoint core with 1 to 4 client connections; per connection its Hello, Welcome and Authenticate at drawn points, `auth` 0 or 1; lists of drawn versions read by the node, lists of no node among them | a connection is sent no Nodes before its first final Authenticate, or its Welcome at `auth` 0, and none while the node holds no list or one of no node; the versions it is sent ascend strictly; it is sent each higher version once; every Nodes is on stream 0 with `flags` 0 | P §5.4; node-discovery, "A client receives the list once it has authenticated"; T 7.3 |
| L7.13 | Inverse | a list and the node's own advertised address, in the list or not | `self` is the index of the node whose address equals the sender's, or `ABSENT` when the list holds none | P §5.4; node-discovery, the same requirement |
| L7.14 | State machine against a model | a node with 0 to 4 client connections and 0 to 4 open transactions, read-write and read-only, begun at drawn times; SIGTERM at a drawn time; the group's answer to the Join of `state` 2 and the close that names it | the node stops no later than `transaction_ms` after it began to drain, whether or not the group answered; within that time it stops no earlier than after each of its client connections was sent a Nodes naming it draining, and once its read-write transactions have ended; a read-only transaction does not hold the stop | node-discovery, "A draining node is announced before it stops"; P §11.14, *Draining*; T 7.3 |
| L7.15 | State machine against a model | the client's machine of this group, with one more command: a node answering a Begin or a one-frame request with code 33, drawn at any point, and lists of drawn versions naming that node serving or draining | a request refused with code 33 is sent to another serving node of the client's tag whenever the model holds one, a one-frame request with the same session and request number, and is returned to the caller only when it holds none; the refusing node is given no new work until a list of a higher version names it serving; the list the client holds is the one it received, unchanged by the refusal | P §9.4; P §11.14, *Draining*; node-discovery, "A draining node is announced before it stops" |

## 8. The cluster end to end

No law of its own. The group adds scenarios and runs over the built binaries (T 8.2 to T 8.4) and no public surface.
What its runs assert is stated as laws where a model can run in seconds: a list's removal and its grace in L3.4, a
session through two nodes in L7.11 and the registry's laws of stage 5.8, the step bound in L5.9. T 8.3's two measures
are figures compared with a spread, corner cases C8.9 and C8.10.

## 9. The benchmark and the exit gate

No law of its own. The group reruns recorded workloads and records figures (T 9.1 to T 9.3); it adds no public surface.

## Found wanting

A weakness in a specification is fixed in the specification first (`tasks.md`). The user ruled on the items on
2026-11-18, and `notes/corner-cases.md`, *Settled in the specification*, holds each ruling. Seven touched a law above:

| Item | Law | The ruling and the law |
|---|---|---|
| 1, the order a merge returns | L5.1, L5.4, L4.4 | P §9.1's order, partition by partition: as the laws were stated |
| 2, what a resume key's `pattern` and `scope` hash | L5.5, L5.6 | P §11.8 states the bytes; the laws draw the two fields and stand |
| 3, the form of a DNS name | L1.8, L2.5 | one encoding, P §5.4; L1.8's generator draws the forms the ruling refuses |
| 4, a Join once every connection has closed | L3.18 | a Join leaves on the first connection that opens: as stated |
| 6, a drain whose announcement is late or cannot be made | L7.14 | `transaction_ms` from the start of the drain wins; L7.14 is written on it |
| 7, the bounds of a client's backoff | L7.10 | bounds stated in P §11.14; L7.10 is written on them |
| 13, `nodes` for a list that changed and changed back | L3.9 | 1 whenever a command changed the list: as stated |
