# Stage 5.8: the corner cases of every group

Task 0.7. The degenerate and boundary inputs of each group, enumerated before its phase 2
(`openspec/protocols/tdd-protocol.md`, "The suite covers corner cases enumerated in advance"). Each case is a unit test
of its group's phase 2. The group's `notes/group-N.md` names, by the case's number here, the test that covers it, when
its phase-2 exit is claimed.

Sources are cited as `P §n` for `specification/binary_protocol/PROTOCOL.md`, as `capability, "requirement"` for the
change's `specs/`, as `D n` for `design.md` and as `T n` for `tasks.md`. A decoder's cases stand in the order of its
body's "Order" line. An unused byte is refused as `ReservedNotZero` naming its frame offset, 128 plus the command-header
offset (P §4.2). "Read" means the view accepts the body and gives the fields back. `T` is `transaction_ms`. A case whose
outcome no text states is listed with "not stated" and its number under *Found wanting*.
`PROTOCOL.md` is cited as it stands on 2026-11-15, with the stage 5.8 text of its section 11.12a folded into
sections 4.8, 11.1, 11.5, 11.12 and 11.13 (T 0.2).

| Group | Cases |
|---|---|
| 1, the wire | 101 |
| 1b, the exchange in two halves | 14 |
| 2, the new settings | 11 |
| 3, the system group | 74 |
| 4, storage hosts the group | 15 |
| 5, the frontend without books | 37 |
| 6, compute dials the group | 5 |
| 7, the switch | 28 |
| 8, the cluster end to end | 7 |
| 9, the benchmark | 2 |

## 1. The wire, `ultragraphdb_protocol_wire`

### Session (534), P §13.23

Order: unused bytes 24–127 zero; `session`; sections.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.1 | byte 24 nonzero; byte 127 nonzero | `ReservedNotZero` | frame offsets 152 and 255 |
| C1.2 | `session` all zero | `FieldValue` | `"session"` |
| C1.3 | byte 24 nonzero and `session` all zero | `ReservedNotZero`, the earlier rule | offset 152 |
| C1.4 | a body holding a section | section 4.1's refusal of a body past its section table | |
| C1.5 | `session` zero but its last byte; `session` all `0xFF` | read | |
| C1.6 | `principal` 0; `principal` 2⁶⁴ − 1 | read: the body states no rule for it | |
| C1.7 | the golden `session.frame` | read, and built again to the same bytes; the reply is the same bytes | |

### Check (535), P §13.24

Order: command header bytes 0–127 zero; sections; section 0 a whole number of 32-byte checks, at least one; each check
in order: bytes 26–31 zero, `session` not all zero and above the one before, `kind`, `outcome`.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.8 | command header byte 0 nonzero; byte 127 nonzero | `ReservedNotZero` | offsets 128 and 255 |
| C1.9 | a body of no section; of two sections | section 4.1's refusal | |
| C1.10 | section 0 of 31, 33 and 48 bytes | `SectionLength` | section 0, unit 32 |
| C1.11 | section 0 empty | `CountMismatch` | `"checks"` |
| C1.12 | byte 26 of a check nonzero; byte 31 nonzero | `PaddingNotZero` | the byte's body offset |
| C1.13 | a check's `session` all zero, at index 0 and at index 2 | `FieldValue` | `"check"`, the index |
| C1.14 | two checks of equal sessions | `FieldValue` | `"check"`, index 1 |
| C1.15 | the second session below the first | `FieldValue` | `"check"`, index 1 |
| C1.16 | sessions differing in the last byte alone, ascending; `00…01 FF…` before `01 00…` | read: the comparison runs byte by byte from the first | |
| C1.17 | `kind` 0, 3 and 255 | `FieldValue` | `"check"`, the index |
| C1.18 | `outcome` 2, 7, 13, 15, 20, 22 and 255 | `FieldValue` | `"check"`, the index |
| C1.19 | `outcome` 0, 1, 12, 14 and 21 | read by the view | |
| C1.20 | a padding byte nonzero and the `session` all zero in one check | `PaddingNotZero`, the earlier rule | |
| C1.21 | check 0 with `kind` 3 and check 1 with a padding byte nonzero | `FieldValue` `"check"`, index 0: checks are taken in order | |
| C1.22 | section 0 of 33 bytes whose first check is all zero | `SectionLength`, before any check | |
| C1.23 | one check; two checks; as many as a frame of 1,048,576 bytes holds | read | |
| C1.24 | the system group's check of a request: an `outcome` of 1, 12, 14 or 21 | `FieldValue` | `"check"`, the index |
| C1.25 | the requester's check of a reply: an `outcome` of 0 | `FieldValue` | `"check"`, the index |
| C1.26 | the requester's check of a reply: a check whose `session`, `principal` or `kind` is not the one sent | `FieldValue` | `"check"`, the index |
| C1.27 | the requester's check of a reply holding fewer or more checks than sent | `CountMismatch`, before any check | `"checks"` |
| C1.28 | the goldens `check.frame` and `check_reply.frame` | read, and built again to the same bytes | |

### Ended (536), P §13.25

Order: command header bytes 0–127 zero; sections; section 0 a whole number of 16 bytes, at least one session; each not
all zero and above the one before.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.29 | command header byte 0 nonzero; byte 127 nonzero | `ReservedNotZero` | offsets 128 and 255 |
| C1.30 | section 0 of 15, 17 and 24 bytes | `SectionLength` | section 0, unit 16 |
| C1.31 | section 0 empty | `CountMismatch` | `"sessions"` |
| C1.32 | a session all zero, at index 0 and at index 1 | `FieldValue` | `"session"`, the index |
| C1.33 | a session equal to the one before; below it | `FieldValue` | `"session"`, index of the later |
| C1.34 | section 0 of 24 bytes whose first session is all zero | `SectionLength`, the earlier rule | |
| C1.35 | one session; two; as many as a frame holds | read | |
| C1.36 | the golden `ended.frame` | read, and built again to the same bytes | |

### Declare (538), P §13.27

Order: unused bytes 2–7 and 48–127 zero; `kind`; `drop`; `built_on`; `commit_version`; sections as section 7.2 states;
for `kind` 1 at least one record, each of kind 8 to 10; for `kind` 2 no record, the term table's count, the term an
IRI; in a `kind` 1 reply `version` and `commit_version` at least 1.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.37 | byte 2, byte 7, byte 48 or byte 127 nonzero | `ReservedNotZero` | offsets 130, 135, 176, 255 |
| C1.38 | `kind` 0, 3 and 255 | `FieldValue` | `"kind"` |
| C1.39 | `kind` 1 with `drop` 1 | `FieldValue` | `"drop"` |
| C1.40 | `kind` 2 with `drop` 0, 5 and 255 | `FieldValue` | `"drop"` |
| C1.41 | `kind` 2 with `built_on` 1 | `FieldValue` | `"built_on"` |
| C1.42 | `kind` 2 with `commit_version` nonzero | `FieldValue` | `"commit_version"` |
| C1.43 | `kind` 1 with a `commit_version` whose low 22 bits are not zero: 1, 2²² − 1, 2²² + 1 | `FieldValue` | `"commit_version"` |
| C1.44 | `kind` 3 with `drop` 9 | `FieldValue` `"kind"`, the earlier rule | |
| C1.45 | a term table broken by section 4.4; names broken by section 4.3; args of 3 bytes | section 7.2's refusals, in its order, before any record's rule | |
| C1.46 | `kind` 1 with no record | `CountMismatch` | `"declarations"` |
| C1.47 | `kind` 1 with a record of kind 7, of kind 11 and of kind 1, at index 0 and at index 2 | `FieldValue` | `"declaration"`, the index |
| C1.48 | `kind` 1 with one record of each of kinds 8, 9 and 10; with one record | read | |
| C1.49 | `kind` 2 with one record | `CountMismatch` | `"declarations"` |
| C1.50 | `kind` 2, `drop` 1 with no term and with two; `drop` 2 with no term and with two | `CountMismatch` | `"drop terms"` |
| C1.51 | `kind` 2, `drop` 3 with one term; `drop` 4 with one term | `CountMismatch` | `"drop terms"` |
| C1.52 | `kind` 2, `drop` 1 whose term is a blank node, a literal, a triple term | `FieldValue` | `"drop term"` |
| C1.53 | `kind` 2 with a record and no term for `drop` 1 | `CountMismatch` `"declarations"`, the earlier rule | |
| C1.54 | a `kind` 1 body with `version` 4 and `commit_version` 0; with `version` 0 and `commit_version` 2²² | `FieldValue` | `"version"` |
| C1.55 | `kind` 1 with `built_on` 0; with `built_on` 2⁶⁴ − 1 | read | |
| C1.56 | a `kind` 2 reply with `version` 0: the bytes of its request; with `version` 5 and `commit_version` 0 | read | |
| C1.57 | a `kind` 1 reply with `version` 1 and `commit_version` 2²² | read | |
| C1.58 | `kind` 1 whose records fill a frame of 1,048,576 bytes | read | |
| C1.59 | the goldens `declare.frame`, `declare_drop.frame` and `declare_reply.frame` | read, and built again to the same bytes | |

### Defaults (539), P §13.28

Order: unused bytes 16–127 zero; sections; the term table; in a request, no entry; in a reply, each entry of kind 1 and
above the one before by its text's bytes, across frames too.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.60 | byte 16 nonzero; byte 127 nonzero | `ReservedNotZero` | offsets 144 and 255 |
| C1.61 | a term table of 25 record bytes; an entry of kind 6 | section 4.4's refusals, before any rule of the body | |
| C1.62 | a request, `position` 0, holding one entry | `CountMismatch` | `"defaults"`, the entries, 0 |
| C1.63 | a reply's entry of kind 2, 3, 4 and 5 | `FieldValue` | `"default"`, the entry's kind |
| C1.64 | a reply's entry below the one before by its text's bytes | `FieldValue` | `"default order"`, the entry's index |
| C1.65 | a reply holding one IRI twice in one frame | `DuplicateTerm`: the term table is checked first (P §4.4, step 9) | |
| C1.66 | a second frame whose first entry equals the first frame's last | `FieldValue` | `"default order"`, the index running on |
| C1.67 | a second frame whose first entry is below the first frame's last | `FieldValue` | `"default order"`, the index running on |
| C1.68 | a second frame with another `tenant`; with another `position`; a frame after the one flagged `LAST`; a second frame of `position` 0 holding an entry, which breaks its own rule and the first's header | `FieldValue` `"defaults"`, the frames taken before it, for the first three; `CountMismatch` `"defaults"` for the last: a frame's own rules come first (P §13.28) | |
| C1.69 | an answer whose bytes pass `catalog_bytes_max` | read no further; the refusal's name is not stated (*Found wanting*, 1) | |
| C1.70 | a reply with no entry, `position` 1 | read: a tenant holding no record | |
| C1.71 | a reply with one entry | read | |
| C1.72 | `B` before `a`; `a` before `ab`; `z` before `é`: the order of the texts' UTF-8 bytes | read in that order, refused in the other | |
| C1.73 | an answer in one frame, flagged `LAST` | read | |
| C1.74 | an answer across two frames; across three; cut so that each frame holds one entry | the first frames without `LAST`, the last with it, one command header, the predicates in order | |
| C1.75 | a frame of an answer holding no entry, between two that hold some | read as the concatenation | |
| C1.76 | the goldens `defaults.frame` and `defaults_reply.frame` | read, and built again to the same bytes | |

### Challenge (519), P §13.8

Order: unused bytes 4–127 zero; `iterations`; sections; section 0 UTF-8; section 1 empty in a request and not empty in
a reply.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.77 | byte 4 nonzero; byte 127 nonzero | `ReservedNotZero` | offsets 132 and 255 |
| C1.78 | `iterations` 1, 4,095, 10,000,001 and 2³² − 1 | `FieldValue` | `"iterations"` |
| C1.79 | `iterations` 4,096 and 10,000,000 with a salt | read | |
| C1.80 | section 0 not UTF-8; ending inside a character | `NotUtf8` | section 0 |
| C1.81 | a request, `iterations` 0, with a salt of one byte | `CountMismatch` | `"salt"`, 1 against 0 |
| C1.82 | a reply with no salt | `CountMismatch` | `"salt"`, 0 against 1 |
| C1.83 | a reply with a salt of 1 byte, of 16 and of 64 | read | |
| C1.84 | a name of no byte; a name not prepared with SASLprep | read: the decoder checks section 0 as UTF-8 alone, and the system group answers it as a name no principal holds | |
| C1.85 | the goldens `challenge.frame` and `challenge_reply.frame` | read, and built again to the same bytes | |

### Verify (520), P §13.9

Order: unused bytes 1–7 and 16–127 zero; `verified`; `principal`; sections; sections 0 and 1 UTF-8; section 2 of 32
bytes; section 3 of 32 bytes in a verified reply, else empty.

| Case | Input | Outcome | Field |
|---|---|---|---|
| C1.86 | byte 1, byte 7, byte 16 or byte 127 nonzero | `ReservedNotZero` | offsets 129, 135, 144, 255 |
| C1.87 | `verified` 2 and 255 | `FieldValue` | `"verified"` |
| C1.88 | `verified` 0 with `principal` 1 | `FieldValue` | `"principal"` |
| C1.89 | `verified` 1 with `principal` 0 | `FieldValue` | `"principal"` |
| C1.90 | section 0 not UTF-8; section 1 not UTF-8; both | `NotUtf8` | section 0 first |
| C1.91 | a proof of 0, 31 and 33 bytes | `CountMismatch` | `"proof"` |
| C1.92 | a verified reply with a signature of 0, 31 and 33 bytes | `CountMismatch` | `"signature"` |
| C1.93 | `verified` 0 with a signature of 32 bytes | `CountMismatch` | `"signature"` |
| C1.94 | a proof of 31 bytes and a signature of 31 bytes | `CountMismatch` `"proof"`, the earlier rule | |
| C1.95 | a request and the reply of a proof that does not hold | the same bytes, read | |
| C1.96 | the goldens `verify.frame` and `verify_reply.frame` | read, and built again to the same bytes | |

### Subscribe, CloseRequest and Catalog, the stage 5.8 fields

| Case | Body | Input | Outcome | Field |
|---|---|---|---|---|
| C1.97 | Subscribe, P §13.3 | byte 9 nonzero; `from` 2⁴²; `latest` 2 and 255; `latest` 1 with `from` 1 and with `from` 5; `from` 2⁴² with `latest` 2 | `ReservedNotZero` at 137; `FieldValue` `"from"`; `FieldValue` `"latest"` for the next three; `FieldValue` `"from"`, the earlier rule | |
| C1.98 | Subscribe | `latest` 1 with `from` 0; `latest` 0 with `from` 0, 7 and 2⁴² − 1; the constructor in use builds `latest` 0 | read | |
| C1.99 | CloseRequest, P §13.16 | byte 4 nonzero; `partition` 0 with `epoch` 5; `partition` 0 with `epoch` 0; `partition` 3 with `epoch` 0 | `ReservedNotZero` at 132; read, by the stage 5.8 constructor; `FieldValue` `"epoch"`; `FieldValue` `"epoch"` | |
| C1.100 | Catalog request, P §13.7 | `version` 0 with `epoch` 0; `version` 0 with `epoch` 9; `version` 3 with `epoch` 0; `version` 3 with `epoch` 9; `version` 0, `epoch` 9 and a byte in section 0 | read; read; read; `FieldValue` `"catalog"`; `CountMismatch` `"catalog"` | |
| C1.101 | Catalog request, P §13.7, §1 | `version` 0 with `epoch` 2⁴² − 1; `version` 0 with `epoch` 2⁴² and with 2⁶⁴ − 1; `version` 3 with `epoch` 2⁴² | read; `FieldValue` `"epoch"` for the next two; `FieldValue` `"catalog"`, the earlier rule | |

The command table: 534, 535, 536, 538 and 539 are commands; 540 is refused as `UnknownCommand` (P §3; wire-protocol, "Command
540 is not served"). Number 537, Roster, is first assigned at stage 5.9 and is refused as `UnknownCommand` before it
(P §3).

## 1b. The exchange in two halves, `ultragraphdb_protocol_scram`

Each case is run through the two halves and through `ScramServer`, and the two agree (D 8; T 1b.5).

| Case | Input | Outcome |
|---|---|---|
| C1b.1 | a name of one character; a name holding `,` and `=`, sent as `=2C` and `=3D` | the relay reads the name `ScramServer::name_in` reads |
| C1b.2 | a name of no character; a name SASLprep changes | as `ScramServer` answers it; the group answers it as a name no principal holds (P §13.8, §13.9) |
| C1b.3 | a client-first-message with an authorization identity; with the `y` and the `p` channel-binding flags; with an extension attribute | refused as `ScramServer` refuses it |
| C1b.4 | a salt of 1 byte, of 16 and of 64 handed to the relay | the server-first-message `ScramServer` builds |
| C1b.5 | the least iteration count `ScramServer` takes, and one below it | built, and refused as `ScramServer` refuses it |
| C1b.6 | a client-final-message whose nonce is the client's alone; another exchange's; one character short | refused as `ScramServer` refuses it |
| C1b.7 | a channel binding other than `biws` | refused as `ScramServer` refuses it |
| C1b.8 | a proof that is no base64; of 31 and of 33 bytes decoded; missing | refused as `ScramServer` refuses it |
| C1b.9 | the right proof with its first bit changed; with its last bit changed | the group's half gives no ServerSignature |
| C1b.10 | the right proof | the ServerSignature `ScramServer` sends, and a `ScramClient` verifies it |
| C1b.11 | a name no principal holds, with any proof | no ServerSignature, as for C1b.9 |
| C1b.12 | the verifier of a name not held, derived twice from one secret; from two secrets; for two names | equal; different; different |
| C1b.13 | the RFC 5802 and RFC 7677 example exchange, user `user`, password `pencil` | the published server-first-message, proof and ServerSignature |
| C1b.14 | a password of no character; a password SASLprep changes | as `ScramServer` answers it |

## 2. The new settings, `ultragraphdb_common_service_config`, `ultragraphdb_common_role`

| Case | Input | Outcome | Source |
|---|---|---|---|
| C2.1 | a storage file with no `[system]` table | read; the instance hosts no group | system-group, "One storage instance hosts the system group" |
| C2.2 | `[system]` with `role_address` and `data_dir` | read as written | D 2 |
| C2.3 | `[system]` with `role_address` and no `data_dir` | refused, exit 78, naming `system.data_dir` | system-group, "A system table without its directory stops the start" |
| C2.4 | `[system]` with `data_dir` and no `role_address` | refused naming `system.role_address` | T 2.4 |
| C2.5 | `[system]` with neither setting, an empty table | refused naming both | T 2.4 |
| C2.6 | `[system]` holding `role_adress` | refused as an unknown setting naming it | role-binaries, "Configuration is explicit and validated at start" |
| C2.7 | `system.role_address` as `tcp://host:port` and as `unix:///path`; one that is no address | read; refused naming the setting | D 2 |
| C2.8 | `cluster.system_address` on a frontend, a compute and a storage file, on either carrier | read; `common_role::system_address` gives it | D 2; T 2.3 |
| C2.9 | a file holding `cluster.frontend_address` and `cluster.system_address` both | read, each as written: the old leaves in group 7 | D 2 |
| C2.10 | a file of the settings in use and none of the new | reads as before this group | T 2.4 |
| C2.11 | `[metadata]` and `[sequencer]` in a storage file, with `[system]` and without | read as written: the config service's System check is group 7's | D 2; T 2.3 |

## 3. The system group, `ultragraphdb_common_system`

### The registry

`sessions_max` is small in each test. Sources: P §11.5, §13.23; system-group, "The registry is asked by
Session, Check and Ended"; transactions, "Exactly-once commits through sessions".

| Case | State | Event | Outcome |
|---|---|---|---|
| C3.1 | an empty registry | a Session | registered, answered with its bytes once durable |
| C3.2 | `sessions_max` − 1 sessions | a Session | registered, none evicted |
| C3.3 | a full registry, none marked | a Session | the least recently registered is evicted, the new one registered |
| C3.4 | a full registry, the oldest marked | a Session | the oldest without a mark is evicted, the marked one stays |
| C3.5 | a registry full of marked sessions | a Session | code 14; nothing registered, nothing evicted |
| C3.6 | C3.5, then an Ended naming one session | a Session | that session is evicted, the new one registered |
| C3.7 | a registry full of sessions whose marks' connections have closed, before twice `T` | a Session | code 14 |
| C3.8 | C3.7 at twice `T` after the close | a Session | the oldest is evicted, the new one registered |
| C3.9 | `sessions_max` 1, its session unmarked; marked | a Session | the one session evicted; code 14 |
| C3.10 | a session held for `alice` | the same Session again | Error 1 naming the session; held once |
| C3.11 | a session held for `alice` | a Session naming it for `bob` | Error 1; the session stays `alice`'s |
| C3.12 | any | a Session naming a principal not held | code 7 naming the principal; nothing registered |
| C3.13 | a session held, for `alice` | a Session naming it for a principal not held; that Session at a full registry holding no session it may evict; one naming a new identity for a principal not held at such a registry | Error 1; Error 1; code 7: the order is Error 1, code 7, code 14, and nothing is evicted (P §13.23) |
| C3.14 | a session evicted, its eviction in no close yet | a Session naming the same identity | registered: the registry does not hold it (P §13.23) |

### Checks and marks

Sources: P §11.5; §13.24; §13.25; system-group, "The registry is asked by Session, Check and Ended" and "A
mark outlives its connection by twice the transaction limit".

| Case | State | Event | Outcome |
|---|---|---|---|
| C3.15 | `alice`'s session, unmarked | a check of kind 2 by `alice` | 1; the session is marked against the connection |
| C3.16 | the same | a check of kind 1 by `alice` | 1; no mark |
| C3.17 | no such session | a check of kind 1; of kind 2 | 12 |
| C3.18 | `bob`'s session, unmarked | a check naming `alice` | 21; no mark |
| C3.19 | no such session | a check naming a principal not held | 12, the first in the order |
| C3.20 | `bob`'s session, marked | a check naming `alice` | 21, before 14 |
| C3.21 | `alice`'s session, marked | a check of kind 2 by `alice`, on the marking connection and on another | 14 |
| C3.22 | `alice`'s session, marked on one connection | a check of kind 1 on another | 14; no mark |
| C3.23 | three sessions | one frame: kind 2 of `alice`'s, kind 2 of one never registered, kind 1 of `bob`'s naming `alice` | one reply, outcomes 1, 12, 21 in that order; `alice`'s alone marked |
| C3.24 | any | a Check frame of one check; of as many as a frame holds | answered whole, every field as sent but `outcome` |
| C3.25 | any | a Check whose check holds `outcome` 1 | refused with `FieldValue` `"check"`, the index; nothing marked |
| C3.26 | a mark on connection A, A open | an Ended on A | the mark ends; no frame answers |
| C3.27 | the same | an Ended on B | passed over; the session stays marked |
| C3.28 | a mark on A, A closed | an Ended on B, before twice `T` | the mark ends; a check of kind 2 then passes |
| C3.29 | a marked session | an Ended naming it twice, in two frames | the first ends the mark, the second is passed over |
| C3.30 | any | an Ended naming a session never registered; a registered one without a mark | passed over |
| C3.31 | marks on A of two sessions, one on B | an Ended on A naming all three | A's two end, B's stays |
| C3.32 | any | an Ended the body's rules refuse | dropped unanswered; no mark ends |
| C3.33 | a mark on A; A closes at `t` | a check of kind 1 on B at `t + 2T` less one tick | 14 |
| C3.34 | the same | a check of kind 1 on B at `t + 2T` | 1 |
| C3.35 | the same | a check of kind 2 on B at `t + 2T` less one tick; at `t + 2T` | 14; 1, marking against B |
| C3.36 | marks on A and B; A closes at `t`, B at `t + T` | checks at `t + 2T` and at `t + 3T` | A's mark is gone at `t + 2T`, B's at `t + 3T` |
| C3.37 | A closes holding no mark | time passes | nothing changes |
| C3.38 | a mark on A; A closes | the group's time reaches `t + 2T` with no command | the mark is gone before the next command is decided |
| C3.39 | a full registry, the oldest marked on A; A closes at `t` | a Session at `t + 2T` less one tick; at `t + 2T` | the next oldest is evicted; the oldest is |

### The quiet time and the restart

Sources: P §11.5; system-group, "A restarted group answers no resend at once". The group begins to listen
at `L`.

| Case | State | Event | Outcome |
|---|---|---|---|
| C3.40 | before `L + 2T` | a check of kind 1 naming a registered, unmarked session of its principal | 14 |
| C3.41 | before `L + 2T` | a check of kind 2 naming such a session | 1; marked |
| C3.42 | before `L + 2T` | a check of kind 1 naming a session not held; another principal's | 12; 21: the order 12, 21, 14 applies in the quiet time (P §11.5) |
| C3.43 | before `L + 2T`, a full registry, none marked | a Session | code 14; nothing evicted |
| C3.44 | before `L + 2T`, a registry not full | a Session | registered |
| C3.45 | at `L + 2T` less one tick | a check of kind 1; a Session needing an eviction | 14; code 14 |
| C3.46 | at `L + 2T` exactly | a check of kind 1; a Session needing an eviction | 1; the oldest evicted: the quiet time is half-open and has ended (P §11.5) |
| C3.47 | one tick past `L + 2T` | a check of kind 1; a Session needing an eviction | 1; the oldest evicted |
| C3.48 | a session registered and marked | the group restarts | the session is held, the mark is gone |
| C3.49 | the cluster's first start | the group begins to listen | the quiet time runs as after a restart |

### While a log syncs

Sources: P §11.5; system-group, "The system group goes on answering checks while a log syncs".

| Case | State | Event | Outcome |
|---|---|---|---|
| C3.50 | the sequencer's sync of a close in flight | a Check and a Session arrive | the Check is answered before the sync ends, the Session after it |
| C3.51 | an eviction appended, its sync in flight | a Check names the evicted session | 12 |
| C3.52 | a registration appended, its sync in flight | a Check names the session with its principal | not 12; whether the principal is read is not stated (*Found wanting*, 2) |
| C3.53 | a registration and the eviction it caused appended, neither durable | a Check naming each | the registered one is held, the evicted one answers 12 |
| C3.54 | a sync in flight | a Declare, a Session and a Lease arrive in that order | each answered after the sync, in that order |
| C3.55 | a registration appended and not durable | the group is killed | after recovery the session is held or absent, and its Session was never answered |

### The catalog, Declare, Defaults and Catalog

Sources: P §11.13; §13.7; §13.27; §13.28; system-group, "The system group builds the catalog
versions it is asked for"; metadata-group, "Catalog versions take effect at an epoch boundary"; schema-changes.

| Case | State | Event | Outcome |
|---|---|---|---|
| C3.56 | a tenant holding no version | a Declare of kind 1, `built_on` 0 | version 1 pending; answered at the next close |
| C3.57 | version 3 current | a Declare of kind 1, `built_on` 2 | code 2, kind 5, naming versions 2 and 3; version 3 stays |
| C3.58 | version 3 current | `built_on` 4; `built_on` 0 | code 2, kind 5; version 3 stays |
| C3.59 | version 4 pending, no close since | a Declare of kind 1, `built_on` 4 | version 5 pending; both take their epoch at the next close |
| C3.60 | version 4 being checked | a Declare of kind 1, `built_on` 4 | code 2, kind 5 |
| C3.61 | a tightening's Declare open | its connection closes; a partition then refuses | a restoring version is held; no Catalog request of `version` 0 naming a closed epoch returns the tightened one |
| C3.62 | a Declare of kind 1 open, tightening nothing | its connection closes before the close; its stream is cancelled | the version takes its epoch at the next close all the same |
| C3.63 | a Declare of kind 2 open | its connection closes | the version stays pending and durable |
| C3.64 | version 3 current | a Declare building a version of 16,777,216 bytes; of 16,777,217; a version of 523,001 bytes read at the floor of `max_frame` | committed; refused with code 7, `detail` 16,777,216; read in two Catalog frames (P §11.9, §13.7; design D13) |
| C3.65 | a predicate not declared | a Declare of kind 2, `drop` 1, naming it | `version` 0 at once, the request's bytes; the current version stays |
| C3.66 | any | a Declare of kind 2 with `drop` 2; with `drop` 3 | `version` 0 |
| C3.67 | a tenant with a schema and default-entry records; with records and the initial catalog | a Declare of kind 2, `drop` 4 | a version of the initial catalog and no record; `version` 0 and no record |
| C3.68 | a tightening being checked over two partitions | one report passed; the second passed; the second silent at `catalog_check_ms`; one tick before it | unanswered; answered; code 24 naming the partition, restoring version held; still waiting |
| C3.69 | a tenant the group does not hold | a Declare; a Defaults | code 7, `detail` the tenant |
| C3.70 | records for no predicate; for `score` and `nick`; for enough to fill two frames at the requester's `max_frame`; three | a Defaults | no entry and `LAST`; `nick` before `score`; two frames, the last alone `LAST`; three |
| C3.71 | the group's latest close `e`, a version in effect from `e` and one from `e + 1` announced | a Catalog request of `version` 0 naming `e` − 1; `e`; `e + 1`; and `epoch` 0 | the version in effect at `e` − 1, the view being behind a close; the one at `e`; Error 1 naming both epochs; the current version, pending or not |
| C3.72 | a tenant holding no version | a Catalog request of `version` 0 | code 7, `detail` 0 |

### CloseRequest at the group

Sources: P §13.16; sequencer, "An early close is asked by CloseRequest from either role".

| Case | State | Event | Outcome |
|---|---|---|---|
| C3.73 | epoch 12 the latest close | a frontend's CloseRequest of `partition` 0 naming epoch 13; naming epoch 12; naming epoch 14 | epoch 13 closes once `epoch_ms_min` has passed; nothing closes; dropped. No frame answers any of the three (P §13.16) |
| C3.74 | the same | a CloseRequest of `partition` 0 on a `storage` connection; of `partition` 3 on a `frontend` connection | dropped unanswered; refused with `FieldValue` `"partition"` and dropped unanswered (P §13.16) |

## 4. Storage hosts the group, `ultragraphdb_common_storage`, `ultragraphdb_storage`, `ultragraphdb_cluster_partition`

Sources: system-group, "The hosting process starts the group first and stops it last", "A lost system group is waited
for until the deadline"; role-binaries, "Readiness, health and shutdown"; D 10; for a Drop's check, P §6.5;
schema-changes, "A partition checks the catalog version a Drop names"; D 14.

| Case | State | Event | Outcome |
|---|---|---|---|
| C4.1 | settings with `[system]` | the process starts | the group recovers and listens, then the partitions start; `/ready` answers with no other role running |
| C4.2 | settings without `[system]` | the process starts | no listener at a system address; the partitions dial `cluster.system_address` |
| C4.3 | the hosting process, one partition | the partition starts | it dials `cluster.system_address` as a partition of another process does |
| C4.4 | the hosting process, its partition's log naming version 3 | a restart | the group recovers first; the partition reads version 3 with Catalog, then replays |
| C4.5 | the hosting process | a write to the metadata log fails; to the sequencer's | an ordinary shutdown, exit 74 |
| C4.6 | the hosting process | compute and the frontend go | it exits for neither |
| C4.7 | a process not hosting | the group gone for `join_deadline_ms` less one tick; for `join_deadline_ms` | it dials on; exit 69 naming the address |
| C4.8 | a process not hosting, the group gone | commits until the open epoch is full; one more | acknowledged; code 5 once `close_request_ms` has passed |
| C4.9 | the hosting process | SIGTERM | the partitions' logs applied and synced, then the group stopped with its logs synced |
| C4.10 | a process not hosting | its subscription is ended as no longer held | exit 69 naming what the group said |
| C4.11 | version 4 in effect at the round's epoch | a Drop of kind 3 naming `catalog` 4; of kind 4 | committed |
| C4.12 | version 4 in effect at the round's epoch | a Drop of kind 3 naming `catalog` 3, one below; of kind 4 | code 2, kind 5; nothing removed |
| C4.13 | version 4 in effect at the round's epoch | a Drop of kind 3 naming `catalog` 5, one above; of kind 4 | code 2, kind 5; nothing removed |
| C4.14 | version 4 taking effect at exactly the round's epoch | a Drop of kind 4 naming `catalog` 4; naming 3 | committed; code 2, kind 5 |
| C4.15 | version 4 in effect at the round's epoch | a Drop of kind 1 with `catalog` 0; of kind 2 | not checked; committed |
| C4.16 | version 4 in effect at the round's epoch | a Drop of kind 1 with `catalog` 0 naming a predicate the tenant does not hold; of kind 2 naming a graph it does not hold | not checked; committed, nothing removed |

## 5. The frontend without books, `ultragraphdb_common_frontend`

### The view

Sources: P §11.12, *Epochs*; §11.13; client-endpoint, "An endpoint core keeps a view of what the closes
announce".

| Case | State | Event | Outcome |
|---|---|---|---|
| C5.1 | a core that has just subscribed, no close yet | a Begin | not answered from the view before it holds a close |
| C5.2 | 40 epochs closed, 30 trimmed | a core subscribes with `latest` | the first close is epoch 40; the view holds it before the first Begin |
| C5.3 | no close exists | a core subscribes with `latest` | the first close is that of epoch 1 |
| C5.4 | the view at close 7, open epoch 8 | a Begin with no token; with a token of epoch 7; of epoch 8 | answered at once |
| C5.5 | the same | a Begin with a token of epoch 9 | held; answered when the close of 8 arrives |
| C5.6 | the same | a Begin with a token of epoch 10 | held until the close of 9 |
| C5.7 | the same, a token of epoch 9 | the deadline passes with no close | code 24, `detail` 0 |
| C5.8 | the same | the close of 8 arrives one tick before the deadline; in the tick of the deadline | answered; answered: a close that has arrived when the deadline is reached is taken first (P §4.8) |
| C5.9 | the same | a DirectWrite naming a token of epoch 9 | goes to its partition, which holds the commit |
| C5.10 | the view at close 7 | the subscription is opened again and its first close is 7 | the view stays at 7 and applies nothing twice |
| C5.11 | the view at close 7 | the subscription is opened again and its first close is 12 | the view holds 12; before it answers from the view the core reads Placement again and, with Catalog, the current version of each tenant the view holds (P §11.12, *Epochs*) |
| C5.12 | the view holds version 3 of `t1`; another core committed version 4 | a declaring Commit built on 3 | the group's code 2, kind 5, passed on; after the close the retry builds on 4 |
| C5.13 | a tenant first used, holding no version | the Catalog read | code 7, `detail` 0: the tenant holds none |
| C5.14 | the view at close `e`, the group at `e + 1` | a Describe | a Catalog request of `version` 0 naming `e`, and a Defaults; the answer of the version in effect at `e` |
| C5.15 | a tightening being checked, asked by another core | a Describe | the version before it |
| C5.16 | a Describe's Defaults answer in two frames; a Catalog version in two frames | the core reads them | one list in order; one version |
| C5.17 | a snapshot read waiting on the open epoch 8 | the core asks | a CloseRequest naming epoch 8 with `partition` 0 |
| C5.18 | a close assigns a placement version | the core reads the close | it reads Placement again |

### The turn

Sources: P §11.5; client-endpoint, "The endpoint asks the system group"; D 7.

| Case | State | Event | Outcome |
|---|---|---|---|
| C5.19 | nothing queued | a turn | no frame |
| C5.20 | one Check queued | a turn | one Check frame of one check |
| C5.21 | eight Commits through eight sessions queued | a turn | one Check frame of eight checks, sessions ascending |
| C5.22 | a Commit and a resend through one session queued | a turn; the next | one check of the session; the second in the next frame |
| C5.23 | an Ended and a Check of one session queued | a turn | the Ended frame, then the Check frame |
| C5.24 | two Endeds of one session queued | a turn | the session once in the Ended frame |
| C5.25 | a request queued while the task sends | the next turn | it leaves then |
| C5.26 | a session's first check unanswered | its second check is queued | it leaves in the next turn, waiting for no reply |
| C5.27 | more checks queued than one frame holds | a turn; the next | one Check frame of as many checks as `max_frame` takes, in ascending order of their sessions; the rest (P §11.5, *Turns*) |
| C5.28 | a check answered 12, 21, 14 | the core answers its request | the client receives that code; no partition sees the request |
| C5.29 | a Recall answered 18 by every partition | the second Check answers 12; 21; 14; 1 | code 12; code 21; code 14; code 18 (P §11.5) |
| C5.30 | a transaction's declarations at the bytes one Declare frame holds; one record past | the Write | taken; code 6, `detail` the bound, the transaction ended |

### The link lost, the relay and the throttle

Sources: P §11.12, *While the system group cannot be reached*; client-endpoint, "Connections authenticate against the
metadata group's principals"; P §11.9.

| Case | State | Event | Outcome |
|---|---|---|---|
| C5.31 | the link closed | a Commit through a session; a Register; a declaring Commit; a Describe | code 17 each; the Commit reaches no partition |
| C5.32 | the link closes with a Check unanswered | the transaction's Bind stream | cancelled; the client answered code 17 |
| C5.33 | the link closed, a Commit at its partition | the partition answers; the core dials again | the client receives the reply; an Ended naming the session leaves on the new connection |
| C5.34 | the link closed | an Authenticate, 80 times from one address; one more once the group answers | Error 17 on stream 0 and the connection closed each time, no failure counted; the last authenticates |
| C5.35 | the link closed | a Begin, a read, a commit through no session | served from the view |
| C5.36 | thresholds 50 and 20 | failures 50, 51, 59, 60, 70 and 71 of one source; a success after the 55th; a second source | no delay at 50; 100 ms after 51; 25.6 s, then 30 s at the cap; a ban past 70; the count cleared; the second source undelayed |
| C5.37 | a banned source | a connection at `auth_ban_ms` less one tick; at `auth_ban_ms`; two IPv6 addresses of one /64; a Challenge answered and the Verify's link lost | closed as accepted; accepted; one source; Error 17 and no failure counted |

## 6. Compute dials the group, `ultragraphdb_common_compute`, `ultragraphdb_compute`

Sources: role-binaries, "Readiness, health and shutdown", "Exit codes name the cause"; system-group, "A lost system
group is waited for until the deadline".

| Case | State | Event | Outcome |
|---|---|---|---|
| C6.1 | no process at `cluster.system_address` | compute starts; storage starts within the deadline | it dials until answered, joins, and `/ready` answers once the group and a partition answer |
| C6.2 | compute serving | the group's connection closes | it dials again |
| C6.3 | the group gone | it returns one tick before compute's deadline; it does not return | compute joins again and serves; exit 69 naming the address |
| C6.4 | compute joined | it reads Members, Placement and Catalog | each at `cluster.system_address` |
| C6.5 | compute started with another role secret | it dials the group | refused with code 19; it retries until its deadline |

## 7. The switch, `ultragraphdb_common_config_check` and the supervisor

Sources: config-service, "The config service validates every setting before it serves one"; D 2; D 11; role-binaries,
"The supervisor starts, stops and reports the roles".

### The System check

| Case | Target | Outcome |
|---|---|---|
| C7.1 | no storage instance holds `[system]` | refused naming `system` and the target |
| C7.2 | `storage_1` and `storage_2` both hold it | refused naming `system` and both instances |
| C7.3 | one of one storage instance holds it; one of eight | accepted |
| C7.4 | the host lacks `system.role_address` | refused naming it |
| C7.5 | the host lacks `system.data_dir` | refused naming it |
| C7.6 | the host's `[system]` table is empty | refused naming both settings |
| C7.7 | the host lacks `[metadata]`; lacks `[sequencer]`; lacks both | refused naming each absent table |
| C7.8 | the host holds no partition | refused |
| C7.9 | a storage instance not hosting holds `[metadata]`; the frontend holds `[sequencer]`; a compute instance holds `[metadata]` | refused naming the table and the instance |
| C7.10 | the host lacks `data_dir` and `storage_2` holds `[metadata]` | refused listing both defects |

### The System address check

The host's `system.role_address` is `tcp://storage-0:7100` or `unix:///run/ultragraph/system.sock`.

| Case | An instance's `cluster.system_address` | Outcome |
|---|---|---|
| C7.11 | equal | accepted |
| C7.12 | `unix://` against the host's `tcp://`: the scheme differs | refused naming both addresses and both instances |
| C7.13 | `tcp://storage-0:7101`: the port alone differs | refused |
| C7.14 | `unix:///run/ultragraph/other.sock`: the path alone differs | refused |
| C7.15 | `tcp://system.svc:7100`: the host alone differs | accepted |
| C7.16 | `tcp://system.svc:7101`: the host and the port differ | refused |
| C7.17 | the hosting instance's own `cluster.system_address` differs in its port | refused |
| C7.18 | an instance holds no `cluster.system_address` | refused by the Required check |

### The Data, Addresses and Fixed checks, and the old settings

| Case | Target | Outcome |
|---|---|---|
| C7.19 | `system.data_dir` equal to the instance's `storage.data_dir` | refused naming both settings and the instance |
| C7.20 | `storage.data_dir = "/data/storage"`, `system.data_dir = "/data/storage/system"`: inside | refused |
| C7.21 | `storage.data_dir = "/data/system/storage"`, `system.data_dir = "/data/system"`: containing | refused |
| C7.22 | `/data/system` and `/data/storage`: siblings | accepted |
| C7.23 | `/data/sys` and `/data/system`: one text a prefix of the other, neither inside the other | accepted |
| C7.24 | `/data/storage/` against `/data/storage`; a path holding `.` or `..` | equal, a trailing separator ignored; compared component by component as written (config-service, Data) |
| C7.25 | `system.data_dir` inside the `data_dir` of an instance on another host | accepted by the Data check, which compares with the `data_dir` of instances on its own host alone (config-service, Data; T 7.3) |
| C7.26 | `system.role_address` equal to a partition's derived address; naming a wildcard host | refused by the Addresses check |
| C7.27 | against the active revision: the instance holding `[system]` differs; its `system.data_dir` differs; its `system.role_address` alone differs; no revision is active | refused; refused; accepted; accepted |
| C7.28 | a file holding `cluster.frontend_address`; the frontend's holding `frontend.role_address`; holding `frontend.data_dir` | refused naming the setting as unknown |

## 8. The cluster end to end, `ultragraphdb_tests`

Sources: T 8.2, T 8.3; system-group, "A lost system group is waited for until the deadline"; client-endpoint, "A
restarted frontend serves the sessions it served before"; kubernetes-deployment.

| Case | Run | Outcome |
|---|---|---|
| C8.1 | the hosting storage process killed with commits in flight at a partition of another process, and restarted | each commit commits once; a resend within twice `T` of the group listening is answered 14, and after it its stored reply or 18 |
| C8.2 | the frontend killed during a tightening's check | the check ends in the group; Describe shows what took effect |
| C8.3 | the frontend's connection to the group closed and dialled again with requests at partitions | each request is answered; each session's next Commit passes its check |
| C8.4 | a frontend started before storage | it dials until answered; its `/ready` answers after storage's |
| C8.5 | the frontend killed and restarted between a session's requests 1 and 2 | request 2 commits; the frontend replayed no log |
| C8.6 | the hosting storage process killed and not back within `join_deadline_ms` | the frontend and compute exit 69 naming the address |
| C8.7 | the hosting process killed; session-less writes to a partition of another process until its epoch is full | acknowledged until then; code 5 after `close_request_ms` |

## 9. The benchmark, `ultragraphdb_tool_bench`

Source: `proposal.md`, *Success*, 5; T 9.3.

| Case | Figure | Outcome |
|---|---|---|
| C9.1 | throughput at saturation at the lower edge of the baseline's spread; below it | within the bound; a regression to root-cause |
| C9.2 | a commit's median latency risen by exactly the median round trip of a Check frame; by more | within the bound; a regression to root-cause |

## Found wanting

A weakness in a specification is fixed in the specification first (`tasks.md`). Each item is a case above whose outcome
no text states, or a rule on which two texts disagree; none is decided here. Each is open, routed to the user.
None touches a law of `notes/laws.md`.

1. **A Defaults answer past `catalog_bytes_max` (C1.69).** Open, routed to the user. P §13.28: the requester "reads no
   further once the answer's bytes pass `catalog_bytes_max`", naming no refusal and not saying which sections' bytes
   count. P §13.7 names it for Catalog: `FieldValue` (`"catalog"`), over section 0.
2. **The principal of a registration not yet durable (C3.52).** Open, routed to the user. P §11.5: "whether the
   registry holds a session is read from every command appended, a registration or an eviction whose sync is still in
   flight included, and everything else from what is durable". A session appended and not durable is held, and its
   principal, which outcome 21 compares, is not durable. system-group, "The system group goes on answering checks while
   a log syncs", repeats the sentence.
