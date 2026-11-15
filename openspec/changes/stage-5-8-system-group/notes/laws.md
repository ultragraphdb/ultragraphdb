# Stage 5.8: the laws of every group

Task 0.6. A law holds for all inputs of a stated kind (`openspec/protocols/tdd-protocol.md`, "Phase 2 states every law
as a property test"). Each law below is one Hegel property test of its group's phase 2: written against the
unimplemented surface, observed failing, under `tests/property/` in the crate's `property` target. Each computes what it
expects from the drawn inputs by a rule independent of the code under test: bytes laid out by hand from the section's
table, a model written from the specification, or `ScramServer` and `ScramClient` as they stand.

The kinds are the protocol's: round trip, refusal, order, inverse, bound, state machine. Generators draw sizes and
counts inside the stated limits and well below them, so each target runs in seconds. A property test replaces no
scenario, no golden frame and no case of `notes/corner-cases.md`.

Sources are cited as `P §n` for `specification/binary_protocol/PROTOCOL.md`, as `capability, "requirement"` for the
change's `specs/`, as `D n` for `design.md` and as `T n` for `tasks.md`.
`PROTOCOL.md` is cited as it stands on 2026-11-15, with the stage 5.8 text of its section 11.12a folded into
sections 4.8, 11.1, 11.5, 11.12 and 11.13 (T 0.2).

| Group | Laws |
|---|---|
| 1, the wire | 26 |
| 1b, the exchange in two halves | 8 |
| 2, the new settings | 6 |
| 3, the system group | 35 |
| 4, storage hosts the group | 1 |
| 5, the frontend without books | 13 |
| 6, compute dials the group | none |
| 7, the switch | 6 |
| 8, the cluster end to end | none |
| 9, the benchmark | none |

## 1. The wire, `ultragraphdb_protocol_wire`

### Round trips

A body built from the drawn fields is read back by its view to the same fields, and builds again to the same bytes. The
expected bytes are laid out by hand from the section's tables.

| Law | Body | Inputs drawn | Source |
|---|---|---|---|
| L1.1 | Session, request and reply | `session` of 16 bytes not all zero; `principal` any u64 | P §13.23; T 1.6 |
| L1.2 | Check request | 1 to 64 checks: distinct sessions sorted byte by byte, each not all zero; `principal` any u64; `kind` 1 or 2; `outcome` 0 | P §13.24; T 1.6 |
| L1.3 | Check reply | the request of L1.2 with each `outcome` drawn from 1, 12, 14, 21 | P §13.24 |
| L1.4 | Ended | 1 to 64 distinct sessions, sorted, each not all zero | P §13.25; T 1.6 |
| L1.5 | Declare of `kind` 1, request | `tenant`, `built_on`, `principal` any u64; 1 to 8 records of kinds 8 to 10 with the term table and names they name | P §13.27, §7.2; T 1.6 |
| L1.6 | Declare of `kind` 1, reply | L1.5 with `version` at least 1 and `commit_version` an epoch of 1 to 2⁴² − 1 shifted left by 22 | P §13.27 |
| L1.7 | Declare of `kind` 2, request and reply | `drop` 1 to 4; one IRI for `drop` 1 or 2, no term for 3 or 4; `built_on` 0; in a reply `version` any u64, `commit_version` 0 | P §13.27 |
| L1.8 | Defaults, request and reply | `tenant` any u64; a reply's `position` at least 1 and 0 to 32 distinct IRIs sorted by their texts' bytes | P §13.28; T 1.6 |
| L1.9 | Challenge, request and reply | a name, UTF-8; a reply's `iterations` 4,096 to 10,000,000 and a salt of 1 to 64 bytes | P §13.8; T 1.6 |
| L1.10 | Verify, request and both replies | a name and an AuthMessage, UTF-8; a proof of 32 bytes; a verified reply's `principal` and a signature of 32 bytes | P §13.9; T 1.6 |
| L1.11 | Subscribe with `latest` | `latest` 0 with `from` 0 to 2⁴² − 1, or `latest` 1 with `from` 0 | P §13.3 |
| L1.12 | CloseRequest with `partition` 0 | `partition` any u32, 0 included; `epoch` at least 1 | P §13.16 |
| L1.13 | Catalog request naming an epoch | `tenant` any u64; `version` 0 with `epoch` 0 to 2⁴² − 1, or `version` at least 1 with `epoch` 0 | P §13.7 |

### Refusals

A valid body is drawn, one rule of its "Order" line is drawn and broken by an edit to the bytes laid out by hand, and
the view refuses it with that rule's refusal, its field and its index, and with no other. The rules of each body are
those `notes/corner-cases.md` lists for it.

| Law | Body | Rule drawn from | Source |
|---|---|---|---|
| L1.14 | Session | an unused byte of 24 to 127; `session` all zero | P §13.23 |
| L1.15 | Check | a command header byte; a section 0 not a multiple of 32; no check; a padding byte of a drawn check; a `session` all zero, repeated or below the one before; a `kind` outside 1 and 2; an `outcome` outside 0, 1, 12, 14, 21. Each of the last four names the check's index | P §13.24; T 1.6; wire-protocol, "A Check's sessions ascend" |
| L1.16 | Ended | a command header byte; a section 0 not a multiple of 16; no session; a session all zero, repeated or below the one before, naming its index | P §13.25; T 1.6 |
| L1.17 | Declare | an unused byte; `kind`; `drop` against `kind`; `built_on` against `kind`; `commit_version` against `kind` and its low 22 bits; no record in `kind` 1; a record of another kind in `kind` 1; a record in `kind` 2; the term count against `drop`; a term that is no IRI; a `kind` 1 reply with `version` or `commit_version` 0 | P §13.27; T 1.6 |
| L1.18 | Defaults | an unused byte; an entry in a request; a reply's entry of kind 2 to 5; an entry at or below the one before by its text's bytes | P §13.28; T 1.6 |
| L1.19 | Challenge | an unused byte; `iterations` of 1 to 4,095 or above 10,000,000; section 0 not UTF-8; a salt in a request; no salt in a reply | P §13.8 |
| L1.20 | Verify | an unused byte; `verified` above 1; `principal` nonzero with `verified` 0; `principal` 0 with `verified` 1; section 0 or 1 not UTF-8; a proof not of 32 bytes; a signature not of 32 bytes in a verified reply, or present in another | P §13.9 |
| L1.21 | Subscribe, CloseRequest and Catalog, the stage 5.8 fields | Subscribe: an unused byte of 9 to 127, `from` above 2⁴² − 1, `latest` above 1, `latest` 1 with `from` nonzero. CloseRequest: an unused byte, `epoch` 0. Catalog request: `version` at least 1 with `epoch` nonzero, section 0 not empty | P §13.3, §13.16, §13.7; wire-protocol, "A Subscribe from the latest close names no epoch", "A Catalog request may name an epoch only without a version" |

### The order, the builders, the frames and the table

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L1.22 | Order | a valid body of any of the seven new bodies; two distinct rules of its "Order" line | with both broken, the refusal is that of the rule earlier in the line | wire-protocol, "The system group's commands are bodies of the protocol" ("Every decoder SHALL enforce its body's order of checks"); T 1.4 |
| L1.23 | Refusal | the fields of any new body, drawn valid or with one rule broken | the builder refuses exactly when the view refuses the same fields laid out by hand, and with the same refusal | wire-protocol, the same requirement ("a builder SHALL refuse to build what its decoder refuses"); T 1.4 |
| L1.24 | Round trip | 0 to 64 distinct IRIs; a frame size from the smallest holding one entry to `max_frame` | the Defaults answer cut at that size reads back as the same predicates in order; every frame is within the size and holds a whole term table and the first's command header; the last alone is flagged `LAST` | P §13.28; T 1.6; wire-protocol, "Defaults continues across frames in order" |
| L1.25 | Refusal | a Check request and its reply; one check of the reply and one of: `outcome` set to 0, its `session`, `principal` or `kind` changed | the requester's check refuses the reply with `FieldValue` `"check"` and that check's index, and a reply holding another number of checks than the request with `CountMismatch` `"checks"`, before any check; the system group's check refuses a request whose drawn check holds an `outcome` other than 0 the same way | P §13.24; wire-protocol, "A Check request holds no outcome" |
| L1.26 | Inverse | a command number, any u16 | the command table reads 534, 535, 536, 538 and 539 as their commands and gives each its number back; it reads 540 as no command, and 537 as none before stage 5.9 | P §3; wire-protocol, "Command 540 is not served"; T 1.3 |

## 1b. The exchange in two halves, `ultragraphdb_protocol_scram`

Every law draws a name, a password, a salt of 1 to 64 bytes, an iteration count kept small by the generator and two
nonces. `ScramServer` and `ScramClient`, whose surfaces this group leaves unchanged, are the independent sources.

| Law | Kind | Assertion | Source |
|---|---|---|---|
| L1b.1 | Inverse | a `ScramClient` holding the drawn password completes its exchange against `ScramRelay` and the group's half of `ScramVerifier`, and verifies the server-final-message | D 8; T 1b.3 |
| L1b.2 | State machine against a model | for a drawn exchange, right or with one message altered, the two halves accept exactly when `ScramServer` holding the same verifier accepts | D 8; T 1b.5 |
| L1b.3 | Round trip | the relay's server-first-message and server-final-message equal `ScramServer`'s byte for byte, given the same nonces, salt, count and signature | D 8; T 1b.5 |
| L1b.4 | Refusal | a proof with one drawn bit changed and a name no principal holds are refused alike: the group's half gives no ServerSignature for either, and nothing it returns tells them apart | P §13.9; system-group, "A client's proof is checked in the system group"; T 1b.5 |
| L1b.5 | Bound | no message the relay builds holds a run of 32 bytes equal to the stored key or the server key, raw or in base64 | client-endpoint, "Connections authenticate against the metadata group's principals"; T 1b.5 |
| L1b.6 | Inverse | the verifier of an unknown name is a function of the secret and the name: derived twice it is equal, and its salt and count equal those `ScramServer` sends for that name under the same secret | P §13.8; metadata-group, "Principals live in the metadata group"; T 1b.3 |
| L1b.7 | Refusal | over drawn bytes, a client-first-message valid or altered, the relay's reader of the name gives what `ScramServer::name_in` gives, the name or the same error | T 1b.3; D 8 |
| L1b.8 | Refusal | over a client-final-message valid or altered in one drawn place (the nonce, the channel binding, the proof's encoding or length, an attribute added), the relay's reader refuses exactly what `ScramServer::last` refuses, with the same error | T 1b.3; D 8 |

## 2. The new settings, `ultragraphdb_common_service_config`, `ultragraphdb_common_role`

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L2.1 | Round trip | a settings file holding a `[system]` table with both settings and `cluster.system_address`, each a `tcp://` or a `unix://` address or a path | the file reads back as drawn | D 2; T 2.4 |
| L2.2 | Refusal | the file of L2.1 with `role_address`, `data_dir` or both removed from `[system]` | it is refused naming each absent setting, `system.role_address` or `system.data_dir`, and no other | system-group, "A system table without its directory stops the start"; T 2.4 |
| L2.3 | Inverse | a file of the settings in use, with and without the new table and setting added | every setting in use reads as it did before this group, whether or not the new ones are present | D 2 ("each beside the setting in use, none removed"); T 2.4 |
| L2.4 | Round trip | `[metadata]` and `[sequencer]` tables with drawn values, in a storage instance's file | they read back as drawn, and equal what the same text reads as in a frontend's file | D 2; role-binaries, "Configuration is explicit and validated at start"; T 2.3 |
| L2.5 | Round trip | a `cluster.system_address` on either carrier | `common_role::system_address` gives the address drawn | D 2; T 2.3 |
| L2.6 | Refusal | a `[system]` table holding one more key, its name drawn from `a`–`z`, `0`–`9`, `_` and neither of the two | it is refused as an unknown setting naming the key | role-binaries, "Configuration is explicit and validated at start" |

## 3. The system group, `ultragraphdb_common_system`

### The registry and the marks, one state machine

One Hegel state machine against a model written from P §11.5 and §13.23 to §13.25. The model holds: the
registered sessions in the order of their registration, each with its principal; the principals; per session at most
one mark, held against a connection; per connection whether it is open and when it closed; the time; the time the
group began to listen. The machine runs over an `OrderedLog` double (D 12), with `sessions_max` drawn from 1 to 8 and
`transaction_ms` a drawn constant.

| Command | Inputs drawn |
|---|---|
| Session | a connection; an identity, new or one the registry holds; a principal, held or not |
| Check | a connection; 1 to 4 checks of distinct sessions, registered or not; a principal, the session's or another; `kind` 1 or 2 |
| Ended | a connection; 1 to 4 sessions, marked or not |
| Close | an open connection |
| Open | a new connection |
| Pass | a span of time, drawn around `transaction_ms` and twice it, the exact boundary and one tick before it among the values |
| Restart | none: the group stops and begins to listen again, its marks gone |
| Sync | holds or releases the metadata log's sync, so commands stand appended and not durable |

After every command the machine's answer equals the model's, and these hold:

| Law | Invariant | Source |
|---|---|---|
| L3.1 | The registry never holds more than `sessions_max` sessions | P §11.5; metadata-group, "The metadata group registers sessions"; T 3.7 |
| L3.2 | A full registry's eviction never takes a marked session | P §11.5; system-group, "The registry is asked by Session, Check and Ended"; T 3.7 |
| L3.3 | An eviction takes the least recently registered session with no mark, and happens only at a Session finding the registry full | P §11.5; metadata-group, "The metadata group registers sessions" |
| L3.4 | A Session registering a new identity for a held principal is answered with its own bytes, and the registry then holds the session for that principal | P §13.23 |
| L3.5 | A Session's refusals follow the order Error 1, code 7, code 14: one naming a held identity is refused with Error 1 whatever principal it names, held or not, and the session keeps its principal; else one naming a principal not held is refused with code 7, at a full registry too; each registers nothing and evicts nothing | P §13.23; system-group, "A held identity is refused whoever holds it", "An unknown principal registers nothing" |
| L3.6 | A Session naming a new identity and a held principal, finding the registry full of marked sessions, is refused with code 14, registering and evicting nothing | P §11.5, §13.23; transactions, "A full registry of commits in flight refuses a Register as busy" |
| L3.7 | A check's outcome is the first that applies of: 12 the registry does not hold the session; 21 it belongs to another principal; 14 it is marked, whatever the check's kind, or it is of kind 1 within the time of L3.12; else 1 | P §11.5, §13.24; system-group, "The registry is asked by Session, Check and Ended"; T 3.7 |
| L3.8 | A Check's reply holds the request's bytes with each `outcome` filled and nothing else changed, the checks decided in their order | P §13.24 |
| L3.9 | A check of kind 2 with outcome 1 marks its session against the connection that sent it; a check of kind 1, and a check of any other outcome, marks nothing | P §11.5, §13.24 |
| L3.10 | An Ended takes away a mark begun on its own connection and a mark whose connection has closed; it passes over a mark of another open connection, a session without a mark and a session the registry does not hold; it is answered by no frame; sent twice, the second changes nothing | P §13.25; system-group, "A mark outlives its connection by twice the transaction limit" |
| L3.11 | A mark whose connection closed at time `t` is held at every time before `t + 2 × transaction_ms` and gone from that time on, unless an Ended ended it earlier | P §11.5; system-group, the same requirement; T 3.7 |
| L3.12 | While the group has listened for less than twice `transaction_ms`, a half-open interval from the instant it begins to listen, no session is evicted, a Session that would need an eviction is refused with code 14, and a check of kind 1 that would otherwise pass has outcome 14, one naming a session not held 12 and another principal's 21; at exactly twice `transaction_ms` the time has ended | P §11.5; system-group, "A restarted group answers no resend at once"; T 3.7 |
| L3.13 | During that time a check of kind 2 naming a registered, unmarked session of its principal has outcome 1 and marks it | system-group, "A restarted group answers no resend at once" |
| L3.14 | After a Restart the registry holds exactly the sessions whose registrations were durable, less the evictions that were durable, and holds no mark | metadata-group, "The metadata group is a log-backed state machine with one replica"; P §11.5 |
| L3.15 | While a sync is held, a check sees a session as held or not from every command appended: a registration appended and not durable is held, an eviction appended and not durable is gone, outcome 12 | P §11.5; system-group, "The system group goes on answering checks while a log syncs" |
| L3.16 | While a sync is held, every Check is answered before it ends, and every Session is answered only after it, in the order the commands came; no Session is answered before its registration is durable | system-group, the same requirement; P §13.23 |

### The catalog, one state machine

One Hegel state machine against a model written from P §11.13, §13.7 and §13.27. The model
holds, per tenant: the versions in order, each pending, in effect from an epoch, being checked, or withdrawn; the
default-entry records; the latest close.

| Command | Inputs drawn |
|---|---|
| Declare of `kind` 1 | a tenant, held or not; `built_on` the current version, one below, one above or 0; 1 to 4 declarations, loosening, tightening or neither |
| Declare of `kind` 2 | a tenant; `drop` 1 to 4; a predicate declared or not |
| Report | a partition; a version being checked; passed or refused |
| Close | none: the sequencer closes its next epoch |
| Pass | a span of time around `catalog_check_ms` |
| DefaultEntry | a tenant and a predicate |
| CloseConnection | a connection holding an open Declare |
| Catalog | a tenant; `version` 0 with an `epoch` of 0 to one above the latest close, or a `version` held or not |
| Restart | none |

| Law | Invariant | Source |
|---|---|---|
| L3.17 | A tenant's versions are numbered from 1 without gap, and each committed version is one above the version it was built on | metadata-group, "Catalog versions take effect at an epoch boundary"; T 3.7 |
| L3.18 | A Declare of `kind` 1 whose `built_on` is not the tenant's current version, pending or not, is refused with code 2, kind 5, and the current version stays | P §11.13, §13.27; system-group, "A version built on a stale one is refused"; T 3.7 |
| L3.19 | A Declare of `kind` 1 built on a version being checked is refused with code 2, kind 5 | P §13.27; schema-changes, "A version built on one being checked is refused" |
| L3.20 | A Catalog request of `version` 0 naming a closed epoch returns the latest version in effect at that epoch, and never one being checked or withdrawn; one naming an epoch above the latest close is refused with Error 1 | P §13.7; system-group, "The system group builds the catalog versions it is asked for"; T 3.7 |
| L3.21 | A Declare of `kind` 1 that tightens nothing is answered at the first close after its version is pending: `version` the committed one, `commit_version` that close's epoch plus one, shifted left by 22 | P §11.13, *Its epoch*; §13.27 |
| L3.22 | A tightening's Declare is answered only once every partition holding the tenant has reported its check passed; a refusing report, or a partition silent for `catalog_check_ms` from the assigning close, refuses it with code 7 or code 24, and the group then holds a version restoring the previous declaration; a report arriving after that changes nothing | P §11.13, *A tightening's check*; schema-changes, "A tightening is checked before it is acknowledged" |
| L3.23 | Declarations tightening one predicate or aspect and loosening another are refused with code 7, `detail` the index of the first that loosens, and build no version | P §11.13; schema-changes, the same requirement |
| L3.24 | A Declare of `kind` 2 that changes nothing in the catalog is answered at once with the bytes of its request and builds no version; one for a Drop of kind 2 or 3 always is | P §13.27; system-group, "A Drop that changes nothing builds no version" |
| L3.25 | The state after a Declare whose connection closed under it, or whose stream was cancelled, equals the state had the connection stayed | P §13.27; system-group, "A check outlives the connection that asked for it" |
| L3.26 | A default-entry record creates no version, is kept by a Drop of kind 1, 2 or 3 and removed by a Declare of `drop` 4 | metadata-group, "Catalog versions take effect at an epoch boundary"; P §13.27 |
| L3.27 | After a Restart a version being checked is still being checked, the reports the log holds count, and the wait of `catalog_check_ms` begins anew | schema-changes, "A restarted host of the system group takes a check up again" |
| L3.28 | A Declare or a Defaults naming a tenant the group does not hold is refused with code 7, `detail` the tenant, and changes nothing | P §13.27, §13.28 |
| L3.29 | No version the group holds is larger than the cap, and a Declare that would build a larger one is refused with code 7, `detail` the cap, the catalog unchanged | metadata-group, "Catalog versions take effect at an epoch boundary"; P §11.13. The cap is `catalog_bytes_max`, 16,777,216 (P §11.9; design D13) |

### The other commands a `SystemSession` serves

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L3.30 | Order | a set of 0 to 64 predicates recorded for a tenant in a drawn order; a frame size | the Defaults answer lists exactly those predicates, in the order of their texts' bytes, across as many frames as the size takes; `position` is at least 1 | P §13.28; system-group, "Defaults lists the recorded predicates in order" |
| L3.31 | Inverse | names held and not held; a restart between two Challenges | a held name's Challenge answers its salt and count; a name not held answers the same salt and count each time, before and after the restart; no two drawn names not held share a salt | P §13.8; metadata-group, "An unknown name keeps one salt across a restart" |
| L3.32 | State machine against a model | a principal and its password; an AuthMessage; a proof, right, altered, or for a name not held | Verify answers `verified` 1, the principal and a signature of 32 bytes exactly when a `ScramVerifier` derived from the password accepts the proof; otherwise `verified` 0, `principal` 0 and no signature, alike for a wrong proof and a name not held | P §13.9; system-group, "A client's proof is checked in the system group" |
| L3.33 | Bound | any sequence of the commands the endpoint serves, drawn | no reply frame holds a run of 32 bytes equal to a principal's stored key or server key | system-group, "No frame carries a stored key" |
| L3.34 | Order | a number of closes, 0 to 16, and how many of them the sequencer still holds; a Subscribe with `latest` 1 | the first close on the stream is the latest durable one, or that of epoch 1 when none exists, and each later close names the epoch one above the one before | P §13.3; sequencer, "A subscriber holding no close begins at the latest" |
| L3.35 | Refusal | a command number, any u16; a role, `frontend`, `compute` or `storage` | a number P §11.12's table of what each endpoint serves does not have that role send the system group is refused as `CommandNotServed` naming it, or as `UnknownCommand` when the table of section 3 holds no such number; a Subscribe with `latest` 1 from `storage` is refused with `FieldValue` `"latest"`; a CloseRequest with `partition` 0 from `storage`, and one with `partition` 1 or above from `frontend`, is dropped unanswered | P §11.12, §2.5, §13.3, §13.16; system-group, "The system group serves the books' role commands at its role address" |

## 4. Storage hosts the group, `ultragraphdb_cluster_partition`

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L4.1 | Refusal | a tenant's catalog history of 0 to 4 versions, each with its effective epoch; a Drop of kind 1 to 4 naming a `catalog` from 0 to one above the highest version, decided in a round of a drawn epoch | a Drop of kind 3 or 4 commits exactly when its `catalog` is the version in effect at the round's epoch, and is otherwise refused with code 2, kind 5, removing nothing; a Drop of kind 1 or 2 is not checked | P §6.5; schema-changes, "A partition checks the catalog version a Drop names"; D 14; T 4.6 |

The group holds this one law. For the rest it wires the storage binary: it starts `start_system` on a runtime of its
own before the partitions, dials `cluster.system_address`, and sets readiness, the stop order and the exit codes (T
4.3). Every such rule is a law of group 2 (the settings) or group 3 (the system group), or a scenario over the built
binaries. `ultragraphdb_common_storage` and `ultragraphdb_storage` gain no `property` target (`tdd-protocol.md`, "A
group without a law writes no property target").

## 5. The frontend without books, `ultragraphdb_common_frontend`

### The view

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L5.1 | Bound | a sequence of closes in order with their announcements, Declare replies and Catalog reads, for 1 to 3 tenants; subscriptions opened again with `latest`, the first close skipping 0 to 4 closes | the view's latest closed epoch never falls, and no tenant's current version falls; after a subscription opened again the core answers nothing from the view before it has read Placement and, with Catalog, the current version of each tenant the view holds, and the view then holds what the skipped closes announced | client-endpoint, "An endpoint core keeps a view of what the closes announce"; P §11.12, *Epochs*; §11.13; T 5.4 |
| L5.2 | State machine against a model | a view at a closed epoch; a Begin naming a token whose epoch is from 0 to 3 above it and a deadline; closes and time passing | the Begin is answered at once when the token's epoch is at most the open epoch; else it is held, answered when the close before the token's epoch arrives, a close arrived when the deadline is reached included, and refused with code 24, `detail` 0, when its deadline is reached with the close not arrived | P §4.8; §11.12, *Epochs*; client-endpoint, "A token ahead of the core's view waits and is not refused"; T 5.4 |
| L5.3 | Bound | a sequence of Begins naming no token, between closes | every Begin is answered from the view and sends no frame on the `SystemLink` | client-endpoint, "A Begin asks no other process" |
| L5.4 | Inverse | a catalog version's bytes | the schema id the view computes is a function of the bytes: equal for equal bytes on any core, and equal to the one `core_catalog`'s `describe_of` gives for them | client-endpoint, the same requirement; D 1, D 6 |

### The turn

A turn is one run of the task that sends on the core's `SystemLink` (P §11.5). The driver of T 5.2 orders it.

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L5.5 | State machine against a model | per turn, 0 to 16 requests through 1 to 4 sessions, each queueing a Check of kind 1 or 2 or an Ended; a `max_frame` taking 1 to 16 checks | each turn sends at most one Ended frame and then at most one Check frame; each frame holds each session at most once, its sessions ascending, and no more than `max_frame` takes, the rest waiting for the next turn; no frame is empty | P §11.5, *Turns*, §13.24, §13.25; client-endpoint, "The endpoint asks the system group"; T 5.4 |
| L5.6 | State machine against a model | the turns of L5.5 | every Check and every Ended queued is sent exactly once; a session's second check of one turn leaves in the next turn's frame; a session's checks leave in the order they were queued; a turn waits for no reply | P §11.5; client-endpoint, "Two requests of one session in one turn take two frames" |
| L5.7 | State machine against a model | requests through sessions; for each, a Check outcome of 1, 12, 14 or 21 | a request is routed to its partition exactly when its check's outcome is 1; otherwise the client is answered with the outcome's code and no partition sees the request | P §11.5; transactions, "Exactly-once commits through sessions" |
| L5.8 | Order | the answers of 1 to 4 partitions to a Recall, each a stored reply, 14, 17, 13, 18 or a closed connection; the second Check's outcome | the client's answer is the first that applies of: a stored reply; 14; 17; 13; and, every partition having answered 18, 12 on outcome 12, 21 on outcome 21, 14 on outcome 14 and 18 on outcome 1. The second Check is sent only in the last case | P §11.5; transactions, the same requirement |

### The link lost, the relay and the throttle

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L5.9 | State machine against a model | the link closing and dialling again; requests through sessions, Registers, Authenticates, Begins, reads and commits through no session; partitions answering meanwhile | while the link is down every request through a session and every Register is answered code 17, and every Authenticate Error 17 on stream 0 with its connection closed; a Begin, a read and a commit through no session go on; the Ended of each request that ended meanwhile is sent exactly once on the next connection | P §11.12, *While the system group cannot be reached*; client-endpoint, "A request that ends during an outage is ended on the next connection" |
| L5.10 | State machine against a model | the exchanges of L1b.2, run through `ClientSession`, `ScramRelay` and a `RoleDouble` playing the group | the endpoint authenticates a client exactly when a `ScramServer` holding the principal's verifier would, and refuses a wrong proof and a name not held with the same code 19 | client-endpoint, "Connections authenticate against the metadata group's principals"; D 8 |
| L5.11 | State machine against a model | per source, failures, successes, Error 17 answers and time passing; thresholds drawn from 1 to 4 | a source's delay and ban follow from its consecutive failures and the thresholds alone: no delay up to the first threshold; past it a delay doubling from 100 ms to at most 30 s; past the second a ban of `auth_ban_ms`, ending on its own; a success clears the count; an Error 17 counts nothing; another source is never delayed | client-endpoint, the same requirement; P §11.9; T 5.4 |
| L5.12 | Bound | the sources of L5.11 as IPv4 addresses and IPv6 addresses | two IPv6 addresses of one /64 are one source, two IPv4 addresses are two | client-endpoint, the same requirement |
| L5.13 | Bound | a transaction's declarations, their bytes from below to above what one Declare frame holds within the link's `max_frame` | a Write taking them past the bound is refused with code 6, `detail` the bound, ending the transaction; no Declare frame built is larger than `max_frame` | P §11.13 |

## 6. Compute dials the group

No law of its own. The group wires the compute binary: it joins and reads Members, Placement and Catalog at
`cluster.system_address`, and exits past its deadline for a lost group (T 6.3). The bodies it sends are the wire's and
unchanged; the address is group 2's. Its crates gain no `property` target.

## 7. The switch, `ultragraphdb_common_config_check`

| Law | Kind | Inputs drawn | Assertion | Source |
|---|---|---|---|---|
| L7.1 | Refusal | a target of one frontend, 0 to 3 compute and 1 to 8 storage instances; per storage instance whether it holds `[system]`, each of its two settings, `[metadata]`, `[sequencer]` and a partition; per frontend and compute instance whether it holds `[metadata]` or `[sequencer]` | the System check accepts exactly the targets in which one storage instance holds `[system]` with both settings, both moved tables and at least one partition, and no other instance holds a moved table | config-service, "The config service validates every setting before it serves one", System; T 7.3 |
| L7.2 | Refusal | the host's `system.role_address` and, per instance, a `cluster.system_address` equal to it or differing in a drawn subset of scheme, host, port and Unix path | the System address check accepts exactly the targets in which every instance's address agrees in scheme, port and Unix path; a host alone may differ | config-service, the same requirement, System address; T 7.3 |
| L7.3 | Refusal | `system.data_dir` and the instances' `data_dir`s, as paths of 1 to 4 drawn components, on one host or two | the Data check refuses exactly when `system.data_dir` is equal to, inside or containing a `data_dir` of an instance on its host, two directories compared component by component as written, a trailing separator ignored | config-service, the same requirement, Data; T 7.3 |
| L7.4 | Refusal | an accepted target as the active revision; a new revision equal to it or differing in one drawn setting | the Fixed check accepts the equal one and one differing in `system.role_address` alone, and refuses one in which the instance holding `[system]` or its `system.data_dir` differs, naming the setting | config-service, the same requirement, Fixed |
| L7.5 | Bound | an accepted target with 1 to 4 independent defects planted, each of a drawn check | the refusal lists every planted defect, each naming its target, instance, setting and rule, and no other | config-service, the same requirement ("SHALL list every defect found, not the first") |
| L7.6 | Refusal | an accepted target with one of `cluster.frontend_address`, `frontend.role_address`, `frontend.data_dir` added to a drawn file | it is refused naming the setting as unknown | config-service, "The frontend's old settings are refused by name"; D 2 |

The supervisor, the chart, the image and the local cluster script hold no law: they order starts and write files (T
7.5), which scenarios cover. `ultragraphdb_supervisor` gains no `property` target in this group.

## 8. The cluster end to end

No law of its own. The group adds scenarios and failure runs over the built binaries (T 8.2, T 8.3) and no public
surface. What its runs assert, exactly-once through a session's resend, is stated as laws L3.1 to L3.16 and L5.5 to
L5.9, where a model can run in seconds.

## 9. The benchmark and the exit gate

No law of its own. The group reruns recorded workloads and records figures (T 9.1 to T 9.3); it adds no public surface.

## Found wanting

A weakness in a specification is fixed in the specification first (`tasks.md`). No item leaves a law above unstated or
stated on one of two readings. `notes/corner-cases.md` lists the open items, which touch no law.
