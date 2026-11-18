# Feasibility of stage 5.9 against the code after stage 5.8

Nov 18, 2026. Assessed at commit `c08bd90e`, the close of stage 5.8.

## 1. Verdict

The change is implementable against the current code once four blockers are resolved. Two are in the change set itself
(the order of its groups, and deltas written before stage 5.8 was finished) and two need code first (tasks 0.6 and 0.7).
Nothing in the code contradicts the design's decisions D1 to D11.

Each blocker's resolution follows the core principles in their order: correctness, then speed, then scale.

## 2. What was checked

| Question | Finding |
| --- | --- |
| Does `PROTOCOL.md` state this stage? | Yes: sections 5.4 (Nodes, 27), 11.12a, 11.14, 13.26 (Roster, 537), and the "From stage 5.9" paragraphs of sections 13.1, 13.2 and 13.4. Task 0.2 folds them in |
| Are commands 27 and 537 free in the wire crate? | Yes. `Command` holds neither; ComputeRead is 523 |
| Do the two crates merge (D1)? | Yes. Neither depends on the other. `common_frontend` holds 62 source files and 9,596 lines, `common_compute` 56 and 5,574. Only `mod.rs` files share a path. Three crates depend on `common_frontend`: itself, the frontend binary and `frontend_tests` |
| Can one executor serve both callers during the stage? | Yes, while it keeps `ComputeCore`'s connections (D2 moved to group 4) |
| Can `RoleLink` carry Scans (D2)? | Yes. Since stage 5.8 it records each stream's requests and refuses a reply none of them is answered by, and it already meters Entries frames. A Scan recorded as a request makes its Entries a served reply |
| Does the client core have a simulated server to extend (D7)? | Yes: `client_core`'s `utils_test/test_server` |
| Where is the name `frontend` compiled in? | `common_config_check` (`ROLES` and four checks), `common_role` (`ROLE_NAMES`), `common_service` (`RoleKind`), `common_service_config` (the settings reader), `common_system` (the role-command table), the supervisor's start and stop orders, and the frontend binary |
| Do the property tests hold on random draws? | Yes: `make property-random` passed 20 of 20 runs on each of the ten targets on 2026-11-18 |

## 3. Blockers

### B1. The groups cannot land in the order written

**Finding.** The tasks gate every group by `make test`, which starts the built binaries. Groups 1 to 3, first in the
order, retire what the relay runs on while the frontend binary still relays reads until groups 5 and 6:

- From stage 5.9 a compute node's Join carries no role address (`PROTOCOL.md` section 13.1), and Members lists storage
  alone (section 13.2). The frontend finds compute by that address in Members.
- Group 2 refuses the role name `frontend` and the `[frontend]` table, which the frontend binary runs under.

**Resolution (design D12, decided by the user on 2026-11-18).** The relay goes first, the switch to two roles second,
the node list third: groups 0, 5, 6, 4, then 1 and 2, 3, 7, 8, 9.

**Why, by the principles.** Correctness decides it. In this order the processes of a cluster speak one protocol at
every commit, and every group passes the tests over the built binaries. The alternative, two gates as stage 5.8 ran,
leaves most of the stage without those tests; stage 5.8's first full gate after such a window found the defects of four
groups at once. The cost is in the plan alone: the role's renaming moves from group 2 into the switch, compute's Join
moves from group 6 into group 3, and D2 becomes group 4.

### B2. The deltas predate the archived specifications

**Finding.** 33 requirements are modified by this change and by stage 5.8. Archiving copies a MODIFIED requirement
whole, so this change's stale text would replace stage 5.8's and drop its scenarios. `notes/rebase-on-stage-5-8.md`
lists each requirement with the scenarios its delta lacks, 21 requirements in all. Some gaps are intended, since this
stage removes the frontend. Others are losses: the five scenarios of task 8.3's readers, Describe's catalog field, the
quiet time's retry.

**Resolution.** Task 0.1, before group 5: each delta is brought to the archived text, and each stage 5.8 scenario is
kept or its removal stated with its reason. A specification is fixed before code is written against it.

### B3. A recorded disagreement between the session registry and its model

**Closed on 2026-11-18 with no change** (`notes/group-0-registry.md`): the property's harness was at fault, and commit
`758163d3` had corrected it before stage 5.8's note was committed. What follows is the assessment as it stood.

**Finding.** Stage 5.8's group 4 recorded that `common_system`'s `test_the_registry_agrees_with_its_model` drew a
counterexample under random seeds: "Session 2 of 1: left Some(Busy), right Some(Registered)", the registry answering
code 14 where the model registers. The group recorded it for the code's owner, and no later note takes it up. The 20
random runs of that target on 2026-11-18 did not draw it again, which shows it is rare, not that it is gone.

**Why it blocks.** This stage sends one session through two compute nodes and asserts exactly-once across them (task
8.2). A registry that may answer Busy for a session it should register is the ground those tests stand on.

**Resolution.** Task 0.6: the shrunk case becomes a unit test. If it fails, the registry or the model is corrected by
`PROTOCOL.md` sections 11.5 and 13.23, test first. If it passes, the test stays as the record that the case holds.

### B4. One rule written three times, about to be written a fourth

**Finding.** The rule for a late frame, a stray frame and a frame on stream 0 (`PROTOCOL.md` section 2.5 step 3,
section 2.6) is written in `RoleClient::arrived` and in storage's `subscribed` and `catalog_frame`. Group 3 adds
Roster, which compute reads from the system group across frames: a fourth reader.

**Resolution.** Task 0.7: one `RoleClient` method holds the rule, and the three callers use it, before group 3. Stage
5.8's audit found each copy wrong in its own way; one copy is checked once.

## 4. Corner cases for group 4

Stage 5.8 found these on compute's `Link`, which group 4 removes. `RoleLink` takes over the reads' streams, so each
becomes a corner case of `RoleLink`, written as a unit test in phase 2 (task 4.1):

1. A Scan whose frame cannot be built or sent leaves no stream registered. On `Link`, `ScanStream::open` returned early
   and left the entry until a frame arrived or the link closed.
2. A write that fails marks the link closed: its streams are answered 17 and the link is dialled again. On `Link`, a
   write error left the link open, with later sends answered 17 and no dial until the reader ended.
3. An Entries frame past its stream's credit closes the link, pinned in the crate's own suite. The played partition
   answers through `serve_connection`, which holds a stream to its credit, so the crate needs a tap that grants more
   credit than compute sent.
4. A frame on a stream whose reader is gone is checked whole before it is dropped.
5. No test assumes which double a request reaches by the order the doubles bound their ports. A cluster lists nodes by
   address, and ephemeral ports wrap.

## 5. What stage 5.8 left, and where it goes

| Left by stage 5.8 | Taken by |
| --- | --- |
| `common_frontend`'s 77 missed mutants and 77 timeouts (backlog item 25) | Task 5.9, after task 5.7 removes the relay's code and its mutants with it |
| `make property-random` not run at the close | Run on 2026-11-18, green; part of task 0.6's gate |
| The registry's counterexample (group 4, finding 5) | Task 0.6 |
| The reader rule in three places (backlog item 23) | Task 0.7 |
| A Scan that could not be sent; a failed write on `Link`; past credit in the crate's suite; which double a read reaches (item 23) | Group 4, section 4 above |
| An Error 7 within a catalog version; `Push::Close` in a stream count (item 23) | Task 3.6 |
| `Server::machine_of`'s idle limit, open on real time (item 23) | Task 6.6 |
| `hold`'s pace, open on real time (item 23) | Tasks 6.4 and 3.7: the asks go with compute's old Join, and what remains takes the crate's clock |
| Another role's table in a settings file (item 23) | Tasks 0.1 and 6.3: refused by name, as a setting no role reads is a mistake the administrator should hear of |
| The unused `ultragraphdb_traits_catalog` dependency (item 23) | Task 5.2 |
| The benchmark's record after the system group's move | Task 9.1 compares with `benches-2026-11-15.md` |

Mutation runs after the merge would otherwise mutate every moved line: `git diff` shows a moved file as new. The runs
set `MUTANTS_UNCHANGED_BY` to the merge's commit, which `scripts/mutants.sh` already supports.

## 6. The backlog against this stage

Taken into this change: items 23 and 25 whole, as section 5 shows.

Left in the backlog, with the reason:

- **Item 18, one Scan for a read.** Group 5 rewrites the read's path, but taking three Scans as one changes
  `PROTOCOL.md` sections 11.12 and 13.13 and what a snapshot read's version means. That is a protocol decision, not a
  by-product. It is best decided with item 15, the one-frame read of stage 7, since both remove a round trip from a read.
- **Item 13, the owner check without a round trip.** Stage 8, as the design's non-goals state.
- **Item 24, the unused-dependency check.** A tool choice outside this stage's code.
- **Items 14, 15, 21 and 22.** Each names its stage.
