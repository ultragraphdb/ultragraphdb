# Stage 5.8, group 1: the wire, `ultragraphdb_protocol_wire`

Base commit `fe0db0a4`. This file records task 1.1, the Ultra Style lift, which is commit `5e7157ec`, and tasks 1.2 to
1.6, whose base is that commit: the decisions written into the specification first, the five phases, the mapping of
every corner case and law to its test, and what was found wanting.

## Task 1.1: the audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: the crate-level rules over all of `src/` (24,791 lines, 1,486 functions at the base), and by review the files
this group changes: `types/command`, `types/subscribe`, `types/subscribe_view`, `types/close_request`,
`types/close_request_view`, `types/catalog`, `types/catalog_view`, `types/catalog_frames`, `types/challenge`,
`types/challenge_view`, `types/verify`, `types/verify_view`, `traits/`, and the helpers the new bodies read through
(`utils/body.rs`, `utils/command_header.rs`, `utils/fields.rs`, `utils/le.rs`, `utils/continuation.rs`,
`utils/write_sections.rs`, `types/term_table_view`). Every `file:line` below is as at `fe0db0a4`: the lift's
insertions moved the lines after them.

How it was measured. Clippy ran under Bazel, through the aspect, since `bazel build` alone runs no lint:

```
bazel build --aspects=@rules_rust//rust:defs.bzl%rust_clippy_aspect --output_groups=clippy_checks \
  //ultragraphdb_protocol/ultragraphdb_protocol_wire:all
```

It takes the workspace's lint table (`aquery` shows each `--deny=clippy::…` of `[workspace.lints]` on the action) and
was shown to bite: a probe function holding an `unwrap` and an unchecked addition failed it, and was removed again. The
lints of Ultra Style's table that the workspace table does not hold were added for the audit with
`--@rules_rust//rust/settings:clippy_flag=-D…`: `clippy::indexing_slicing`, `clippy::cast_possible_truncation`, and
the two that measure a rule the table states in words, `clippy::must_use_candidate` (*Types*, "`#[must_use]`") and
`clippy::missing_errors_doc` (*Comments and documentation*, "`# Errors` for each error it returns"), with
`clippy::missing_panics_doc`.

One limit of the measure: the aspect reads rules_rust's own `.clippy.toml`, not the repository's, because the root
`BUILD.bazel` does not export `clippy.toml`. `too-many-lines-threshold = 70` therefore did not apply, and rule 4 was
measured by a script counting each function's body lines, comments and blank lines left out.

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow | conforms | no `dyn`, no stored callback, no recursion: a triple term's nesting is computed over the table's earlier entries in one pass and refused past `MAX_TERM_DEPTH` (`types/term_table_view/mod.rs:77-113`, `types/term_table/mod.rs:71`) |
| 2, bounds | conforms | three `while` loops, each stating the progress that ends it: `types/object_records.rs:60`, `types/key_values/mod.rs:55` (8 bytes or more a turn), `types/statement_cache/mod.rs:105` (one statement a turn); `utils/continuation.rs:157` counts its frames against the answer's bytes; `CatalogFrames` and `DescribeFrames` stop at `CATALOG_BYTES_MAX`; `clippy::infinite_iter` and `maybe_infinite_iter` clean |
| 3, planes | **gap, closed in part** | no module stated a plane (0 of 293 files). `src/lib.rs` now states it, as `ultragraphdb_cluster_partition` does. Open: the read path allocates per frame (see *Left open*, 1) |
| 4, 70 lines | conforms | longest bodies 70 (`utils/changes.rs:358`, `check`) and 69 (`types/quad_pattern/mod.rs:104`, `read`); 0 of 1,486 above 70 |
| 5, invariants | conforms | no run-time `assert!` under `src/`; 42 compile-time checks (`const _: () = assert!(…)`) of sizes and constants. The crate holds no state whose violation it could detect: a view is a function of its frame |
| 6, smallest scope | conforms | `clippy::shadow_unrelated` clean |
| 7, results and panics | conforms | no `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` or `unreachable!` under `src/`; `clippy::indexing_slicing` clean (the 15 index expressions left are constant indices into fixed arrays or `as_chunks` pairs, checked at compile time); `#![deny(clippy::arithmetic_side_effects)]` at `src/lib.rs` clean. Two `.ok()` drop an error on purpose, each with its reason, known from stage 5.5: `types/term_table_view/mod.rs:318`, `types/error_reply/error_reply_body_builder.rs:69` |
| 7, arithmetic on peer input | conforms | one unchecked addition on a peer's value, `types/entries_sequence.rs:98` (`view.through() as usize + 1`), cannot overflow: `check_range`, called before it, has refused a `through` at or past `bounds.len()` (`:177`). Its comment states the cast and not this bound; the code is unchanged |
| 8, no macros, `cfg` | conforms | no `macro_rules!`, no `#[cfg]` |
| 9, ownership | conforms | no `unsafe`, `Arc`, `Rc`, `RefCell`, `Cell`, `Mutex` or atomic; 102 `Copy` derives, 38 types asserted at most 64 bytes at compile time (stage 5.5, owner decisions 43 to 46). Of this group's types: `Subscribe` 8 bytes, `SubscribeView` 8, `CloseRequest` 16, `CloseRequestView` 16, `CatalogView`, `ChallengeView` and `VerifyView` asserted |
| 10, zero warnings | conforms | the aspect is clean over the library, and with the workspace table over the unit tests and the BDD target; no `#[allow]`, no `#[expect]`, no `Exception (Ultra rule` |
| Types: explicit widths, casts | conforms | 198 `as` casts, none narrowing (`clippy::cast_possible_truncation` clean); the widenings between `u32`, `usize` and `u64` rest on `utils/constants.rs:8-11`, which makes the width the compiler's to check (`usize::BITS` 32 to 64) |
| Types: exhaustive matches | conforms | `clippy::wildcard_enum_match_arm` clean; by the lint, none of the 33 `_ =>` arms is over an enum |
| Types: `#[must_use]` | **gap, closed** | 374 public functions returning a value that is no `Result` carried none, in 82 files (list below) |
| Comments: `# Errors` | **gap, closed** | 20 public functions returning a `Result` had no `# Errors` section (list below) |
| Naming | gap, left open | see *Left open*, 2 |
| Division | gap, left open | see *Left open*, 3 |

### The lift

Large by the group's measure: 86 files, 492 lines added and 36 removed, none changing what the crate does. So the
group made the lift alone and stopped (tasks.md, 1.1: "in its own commit").

1. `src/lib.rs`: the crate's plane stated in its documentation.
2. `#[must_use]` on the 374 functions `clippy::must_use_candidate` names. Lines as at `fe0db0a4`:

- `errors/statement_error/mod.rs`: 121, 134
- `types/action.rs`: 48, 57
- `types/authenticate/mod.rs`: 49, 57, 62, 67, 72, 77, 82
- `types/authenticate_view/mod.rs`: 27, 32, 37, 42, 47, 52
- `types/begin/mod.rs`: 39, 72, 77, 82
- `types/begin_view/mod.rs`: 28, 33, 38
- `types/bind/mod.rs`: 83
- `types/bind_view/mod.rs`: 42
- `types/catalog/mod.rs`: 39, 51, 62
- `types/catalog_announcement.rs`: 19, 24
- `types/catalog_view/mod.rs`: 39
- `types/chunk_layout/mod.rs`: 26, 64, 69, 74, 83, 88
- `types/chunked_body/mod.rs`: 74, 79
- `types/column_type.rs`: 58, 68
- `types/command/mod.rs`: 162
- `types/commit/mod.rs`: 82, 87, 98
- `types/commit_view/mod.rs`: 35, 40, 51, 56
- `types/compute_read/mod.rs`: 181
- `types/compute_read_view/mod.rs`: 68
- `types/conditional_write/mod.rs`: 92
- `types/conditional_write_view/mod.rs`: 47
- `types/conflict_kind.rs`: 44, 53
- `types/credit/mod.rs`: 22, 27
- `types/credit_view/mod.rs`: 19
- `types/declared_entity_type.rs`: 25, 30, 35
- `types/declared_predicate.rs`: 34, 39, 44, 57, 62, 70, 87, 92, 97, 102, 107, 112, 117, 122, 127
- `types/declared_relation.rs`: 23, 28
- `types/default_entry/mod.rs`: 46
- `types/default_entry_view/mod.rs`: 25
- `types/describe/mod.rs`: 57, 72, 77, 82, 87, 92, 97, 102, 107, 112, 118, 124, 130, 136, 142, 148, 155
- `types/describe_view/mod.rs`: 34, 39, 44, 70
- `types/direct_write/mod.rs`: 43, 89
- `types/direct_write_view/mod.rs`: 42
- `types/drop_command/mod.rs`: 66, 71, 85, 91, 96, 101, 125, 130
- `types/drop_command_view/mod.rs`: 27, 32, 37, 54, 59
- `types/drop_kind.rs`: 36, 45, 52
- `types/error_code/mod.rs`: 154, 164, 174
- `types/error_reply/mod.rs`: 51, 57, 62, 68, 86, 97, 105, 111
- `types/error_reply_view/mod.rs`: 23, 28, 33, 39
- `types/execute/mod.rs`: 41, 60, 65, 70, 75, 80, 85
- `types/execute_view/mod.rs`: 23, 28, 33, 38, 43, 48
- `types/frame_view/mod.rs`: 70, 75, 80, 85
- `types/header/header_encode.rs`: 22
- `types/header/mod.rs`: 50, 66, 73, 79, 85, 91, 97, 103, 109, 115, 121, 126, 131, 136, 141, 146, 151, 156, 161, 166
- `types/header_view/mod.rs`: 62, 67, 72, 77, 82, 87, 92, 97, 102, 107, 112, 117, 122, 127
- `types/hello/mod.rs`: 49, 54, 59, 64, 69
- `types/hello_view/mod.rs`: 23, 28, 33, 38, 43
- `types/key_values/mod.rs`: 76, 81
- `types/link.rs`: 17, 26, 35, 40, 45
- `types/operation/mod.rs`: 33, 38, 43
- `types/operation_result.rs`: 33, 60
- `types/operation_view/mod.rs`: 23, 28, 33, 38, 43, 48
- `types/placement/mod.rs`: 106
- `types/placement_view/mod.rs`: 76
- `types/prepare/mod.rs`: 64, 130, 135, 140, 151, 174, 180
- `types/prepare_view/mod.rs`: 37, 42, 47, 58, 75, 81
- `types/quad_pattern/mod.rs`: 27, 42, 47, 52, 57, 62, 67, 75
- `types/read/mod.rs`: 41, 58, 63, 68, 73, 78
- `types/read_view/mod.rs`: 23, 28, 33, 38, 43
- `types/record_entry.rs`: 18, 23, 28
- `types/reference/mod.rs`: 109
- `types/reference_view/mod.rs`: 29
- `types/report/mod.rs`: 41, 76, 96
- `types/report_view/mod.rs`: 39
- `types/result_batch/mod.rs`: 85, 212, 218, 223, 228, 234, 246, 288
- `types/result_term.rs`: 41, 115, 121, 130, 135
- `types/result_term_view.rs`: 53, 60, 75, 80, 85
- `types/result_view/mod.rs`: 37, 42, 48, 60, 65, 95
- `types/round_body/mod.rs`: 88, 100, 105
- `types/round_header/mod.rs`: 47, 98, 117, 122, 127, 132, 137
- `types/round_view/mod.rs`: 30, 35, 40, 51, 63
- `types/statement_cache/mod.rs`: 43, 194, 199, 204
- `types/tenant_run.rs`: 21, 26, 37
- `types/term_table/mod.rs`: 62, 131, 136, 141, 146
- `types/term_table_view/mod.rs`: 268, 273, 278, 311
- `types/welcome/mod.rs`: 71, 77, 83, 89, 95, 111, 116, 121, 126, 131, 136, 141, 146, 151, 156
- `types/welcome_view/mod.rs`: 27, 32, 37, 42, 47, 52, 57, 62, 67, 72
- `types/write/mod.rs`: 36, 41, 46
- `types/write_set_record/mod.rs`: 85, 114, 122, 128, 222, 233, 244, 249, 284, 289, 294, 299, 304, 315
- `types/write_set_record_view/mod.rs`: 241, 252, 263, 274, 279, 326, 331, 336, 341, 346, 351
- `types/write_view/mod.rs`: 25, 30, 35, 40, 45
- `utils/checksum.rs`: 8, 13

3. A `# Errors` section on 20 functions, the refusals a summary already listed moved under it and none invented:
   `traits/body_builder.rs:33` (`frame`); `traits/body_view.rs:24` (`from_frame`), `:28` (`to_body`);
   `types/chunk_layout/mod.rs:37` (`of_body`); `types/chunked_body/mod.rs:30` (`verify`); `types/command/mod.rs:214`
   (`from_number`); `types/frame_view/mod.rs:28` (`validate`), `:39` (`validate_client`);
   `types/header/header_encode.rs:51` (`stamp_sequence`), `:112` (`encode_frame`);
   `types/header_view/header_view_verify.rs:21` (`verify`), `:27` (`verify_client`); `types/header_view/mod.rs:37`
   (`new`); `types/prepare_view/mod.rs:93` (`check_request`), `:109` (`check_reply`); `types/result_batch/mod.rs:107`
   (`push_row`); `types/statement_cache/mod.rs:55` (`prepare`), `:125` (`execute`); `types/term_table/mod.rs:71`
   (`intern`); `types/welcome/mod.rs:163` (`answer`).

### Evidence

| | Before, at `fe0db0a4` | After the lift |
|---|---|---|
| `:ultragraphdb_protocol_wire_tests` | 829 passed | 829 passed |
| `:bdd` | 62 scenarios, 124 steps passed | 62 scenarios, 124 steps passed |
| Clippy aspect, the workspace table | clean | clean |
| `must_use_candidate`, `missing_errors_doc` | 374 and 20 | 0 and 0 |
| `indexing_slicing`, `cast_possible_truncation`, `missing_panics_doc` | 0, 0, 0 | 0, 0, 0 |
| `cargo fmt -p ultragraphdb_protocol_wire -- --check` | | unchanged |
| `make build` (`bazel build //...`) | | passes: no crate discards a value the 374 attributes now guard |

Not run for the lift: `bazel test //... --test_tag_filters=-integration_test`. The lift adds attributes and
documentation, and `make build` compiles every test target of the workspace against them.

### Left open

1. **Rule 3, memory allocated at startup.** The read path allocates per frame: `TermTableView::read`
   builds its depths and its duplicate index (`types/term_table_view/mod.rs:218`), `utils/body.rs:88` returns a
   section's u32s as a `Vec`, `utils/changes.rs` and `utils/commit_operation_layout.rs` collect records, and every
   builder returns a new `Vec` (`traits/body_builder.rs`, `body` and `frame`). Each is bounded by its frame, 1 MiB.
   Closing it means views and builders taking their buffers from the caller, which changes the two traits and every
   caller in the workspace. Stage 5.5 left the same item open (its `notes/group-12.md`, "the planes of rule 3").
2. **Naming, "units and qualifiers last".** Six public constants put the qualifier first:
   `utils/constants.rs:20` `MAX_FRAME_SIZE`, `:24` `MAX_FRAME_FLOOR`, `:27` `MAX_FRAME_SIZE_U32`, `:31`
   `MAX_FRAME_FLOOR_U32`, `:69` `MAX_EPOCH`, `:84` `MAX_TERM_DEPTH`. Other crates name them, so renaming is not this
   group's (design D2).
3. **Division states its rounding.** 19 divisions of a section's length by its record size do not say they are exact:
   `types/object_record.rs:38`, `types/closed_view/mod.rs:28`, `types/term_table_view/mod.rs:269`,
   `types/write_view/mod.rs:36`, `types/scan_view/mod.rs:75`, `types/scan_view/scan_view_body_view.rs:189`,
   `types/install_view/mod.rs:47`, `types/join_view/join_view_body_view.rs:56`, `types/commit_view/mod.rs:52`,
   `types/reference_view/mod.rs:42`, `types/write_set_record_view/mod.rs:304`, `:316`, `:347`,
   `types/warnings/mod.rs:70`, `types/describe_view/mod.rs:71`, `:77`,
   `utils/commit_operation_layout.rs:293`, `utils/write_sections.rs:59`. Each divides by a nonzero constant a length
   its view's constructor refused unless it was a whole multiple (`SectionLength`), so none rounds. No lint measures
   the rule; the sites are unchanged.
4. **The repository's `clippy.toml` under Bazel.** The aspect cannot read it until the root `BUILD.bazel` exports it.
   Not this crate's to change.

## Why the lift is a commit of its own

Two reasons, either sufficient. The second is settled under *Decided*, 1.

1. The lift is large, and the tasks give it a commit of its own.
2. **Phase 1 cannot land inside this crate alone.** The rows for 534 to 536, 538 and 539 are five new variants of
   `Command`. Three `match`es in two other crates name every variant with no wildcard, as
   `clippy::wildcard_enum_match_arm` has them do, and stop compiling the moment a variant is added:
   - `ultragraphdb_protocol/ultragraphdb_protocol_transport/src/types/server_connection/request_kind.rs:35`,
     `RequestKind::of`: the five join the arm ending `| Command::Held => Self::Open`, as Catalog and CloseRequest
     are there.
   - `ultragraphdb_protocol/ultragraphdb_protocol_transport/src/types/client_authentication/mod.rs:66`,
     `ClientAuthentication::receive`: the five join the arm that refuses with `unexpected_command`.
   - `ultragraphdb_client/ultragraphdb_client_core/src/utils_test/test_server/test_server_requests.rs:62`,
     `TestServer`'s requests: the five join the role commands' empty arm.

   Fifteen patterns, each in an arm that exists, and no other line. tasks.md 1.3 and design D2 have no caller change
   in this group, so this is the user's to decide before phase 1 starts.

## Decided

Decided by the user on 2026-11-15, each written into the specification before any code (step 0 of tasks 1.2 to 1.6).

1. **The matches over `Command` outside the crate.** The existing constructors and accessors keep their signatures, and
   the exhaustive matches over `Command` in `ultragraphdb_protocol_transport` and `ultragraphdb_client_core` take the new
   numbers in this group: tasks.md, group 1's title, task 1.3 and the waves paragraph; design D2, "The new settings
   arrive first and the old leave last".
2. **A builder refuses what its decoder refuses.** Building the frame of a body its decoder refuses gives the decoder's
   refusal and no frame, for the seven bodies of this group: wire-protocol, "The system group's commands are bodies of
   the protocol"; the contract is `BodyBuilder`'s documentation (`src/traits/body_builder.rs`).
3. **A Catalog request's `epoch` out of range** is refused with `FieldValue` naming `epoch`: `PROTOCOL.md` §13.7's Order
   line, after the rule of a request naming a `version`; case C1.101.
4. **A continued Defaults frame** is checked by its own rules first, then against the first frame's command header
   (`FieldValue` `"defaults"`, the frames taken before it), then its first entry against the frames before, as
   `CatalogFrames` orders a Catalog frame's: `PROTOCOL.md` §13.28; case C1.68.
5. **`CloseRequestView` reads `partition` 0.** Which sender may name which `partition`, and which may send a Subscribe's
   `latest` 1, is the receiving session's rule, in group 3: `PROTOCOL.md` §13.16 and §13.3, after each Order line.

## Tasks 1.2 to 1.6: the five phases

Base `5e7157ec`. Targets: `:ultragraphdb_protocol_wire_tests` (unit), `:property` (Hegel, tagged `property-test`) and
`:bdd` of `//ultragraphdb_protocol/ultragraphdb_protocol_wire`.

### Phase 1: the surface

New bodies, each a builder and a view: `Session`/`SessionView` (534); `Check`/`CheckView` (535) over `SessionCheck`,
`CheckKind` and `CheckOutcome`, with the receiver's `check_request` and the requester's `check_reply`;
`Ended`/`EndedView` (536); `Declare`/`DeclareView` (538) of both kinds; `Defaults`/`DefaultsView` (539) with
`Defaults::frames` and `DefaultsFrames`. The command table's five variants; 537 and 540 stay unknown numbers. Beside
the constructors in use: `Subscribe::at_latest` and `latest`, `SubscribeView::latest` and `check_sender`,
`CloseRequest::frontend`, `Catalog::request_at`. Every new body was a bare `unimplemented!()`; the changed rules of
existing decoders were stubbed at their new branch alone (a Subscribe's byte 8, a CloseRequest of `partition` 0, a
Catalog request naming an epoch, a verified reply of `principal` 0, the five numbers of the table), and the `frame`
of `Challenge` and `Verify`. `bazel build //...` passed on the surface with the three matches of *Decided*, 1, and
no other crate failed to build.

### Phase 2: the suite, observed failing

Run on the phase-1 surface, `bazel test //ultragraphdb_protocol/ultragraphdb_protocol_wire:all`:

| Target | Run | Failed | Passed |
|---|---|---|---|
| unit | 969 tests | 145, each the `not implemented` panic | 824 |
| property | 40 tests | 38, each the `not implemented` panic | 2 |
| bdd | 73 scenarios, 136 steps | 10 scenarios, each step the `not implemented` panic | 63 |

Before the group the unit target ran 829 tests and the BDD target 62 scenarios (124 steps); the crate had no property
target. Of the 145 failing unit tests 11 are tests the crate had, which cross a stubbed branch: the command table's five
sweeps, `utils::constants_tests::test_reserved_numbers_belong_to_no_command`, the golden Challenge and Verify builders
and views (their `frame` was stubbed), and Catalog's request test.

What passed although written in phase 2, and why: the surface under each exists already and holds the rule, so no
body was there to stub.

- Unit, 8: `challenge_view_tests::test_a_name_ending_inside_a_character_is_refused` and
  `test_a_request_with_a_salt_of_one_byte_is_refused` (C1.80, C1.81: `ChallengeView` already read §13.8 as it
  stands); `verify_view_tests::test_verified_is_a_boolean`, `test_section_0_is_checked_before_section_1`,
  `test_a_proof_of_no_byte_and_a_signature_of_33_are_refused`, `test_the_proof_is_counted_before_the_signature` and
  `test_a_name_of_no_byte_reads` (C1.87, C1.90 to C1.92, C1.94: rules `VerifyView` already held);
  `check_outcome_tests::test_all_lists_every_outcome_in_number_order`, which reads a constant.
- Property, 2: `challenge_view_property_tests`' refusal and order laws (L1.19 and L1.22 for Challenge), over the
  decoder as it stood.
- BDD, 1: "Command 540 is not served": 540 was an unknown number before, and stays one.

The golden frames `session.frame`, `check.frame`, `check_reply.frame`, `ended.frame`, `declare.frame`,
`declare_reply.frame`, `declare_drop.frame`, `defaults.frame` and `defaults_reply.frame` come from
`testdata/golden/roles/generate_roles.py`, extended and committed with them; `tests/utils/golden_roles.rs` holds the
size and checksums it printed. A run of the generator leaves every frame it wrote before unchanged, `challenge.frame`,
`challenge_reply.frame`, `verify.frame` and `verify_reply.frame` among them: §13.8 and §13.9 changed no offset, no
width and no section, and the verified golden names principal 7, so the four need no regeneration.

The scenarios: `features/wire-protocol/system_group.feature` holds the eleven scenarios of "The system group's commands
are bodies of the protocol" verbatim, bound in `tests/system_steps.rs`. The scenarios of the two modified
requirements whose decision this crate makes were in its features before, and their text is unchanged by the delta;
"A Describe continues across frames" and "A Describe past the catalog's bound is refused" are group 5's (tasks.md,
5.1). Two steps end in words a step of the crate already bound, and one step answers both: "it is refused with
`FieldValue` naming `catalog`" (`ugs_steps::catalog_refused`) and "it is refused as an unknown command"
(`bdd::refused_unknown`, whose count of every unassigned number moved into its `When`).

### The corner cases and their tests

Unit tests under `tests/types/<type>/<type>_tests.rs`; a case of a view is in the view's file unless a file is named.

| Case | Test |
|---|---|
| C1.1 | `session_view`: `test_unused_bytes_are_refused` |
| C1.2 | `session_view`: `test_a_session_of_all_zero_is_refused`; `session`: `test_the_builder_refuses_a_session_of_all_zero` |
| C1.3 | `session_view`: `test_an_unused_byte_is_refused_before_the_session` |
| C1.4 | `session_view`: `test_a_body_holding_a_section_is_refused_after_the_session` |
| C1.5 | `session_view`: `test_a_session_of_one_nonzero_byte_reads` |
| C1.6 | `session_view`: `test_any_principal_reads` |
| C1.7 | `session_view`: `test_the_golden_session_reads_and_builds_again`; `session`: `test_a_session_builds_the_golden_frame` |
| C1.8 | `check_view`: `test_a_command_header_byte_is_refused` |
| C1.9 | `check_view`: `test_a_body_of_another_number_of_sections_is_refused` |
| C1.10 | `check_view`: `test_a_section_of_a_broken_length_is_refused` |
| C1.11 | `check_view`: `test_no_check_is_refused`; `check`: `test_the_builder_refuses_no_check` |
| C1.12 | `check_view`: `test_a_padding_byte_is_refused_at_its_body_offset` |
| C1.13 | `check_view`: `test_a_session_of_all_zero_is_refused_with_its_index` |
| C1.14, C1.15 | `check_view`: `test_sessions_that_do_not_ascend_are_refused` |
| C1.16 | `check_view`: `test_sessions_compare_byte_by_byte_from_the_first` |
| C1.17 | `check_view`: `test_a_kind_outside_its_set_is_refused` |
| C1.18 | `check_view`: `test_an_outcome_outside_its_set_is_refused` |
| C1.19 | `check_view`: `test_every_outcome_and_kind_of_the_set_reads` |
| C1.20 | `check_view`: `test_the_padding_is_checked_before_the_session` |
| C1.21 | `check_view`: `test_checks_are_taken_in_order` |
| C1.22 | `check_view`: `test_the_section_length_is_checked_before_any_check` |
| C1.23 | `check`: `test_a_frame_holds_from_one_check_to_as_many_as_fit` |
| C1.24 | `check_view`: `test_a_request_holding_an_outcome_is_refused_by_its_receiver` |
| C1.25 | `check_view`: `test_a_reply_without_an_outcome_is_refused_by_the_requester` |
| C1.26 | `check_view`: `test_a_reply_of_another_check_is_refused_by_the_requester` |
| C1.27 | `check_view`: `test_a_reply_of_another_count_is_refused_before_any_check` |
| C1.28 | `check_view`: `test_the_golden_checks_read_and_build_again`; `check`: `test_a_check_builds_the_golden_request_and_reply` |
| C1.29 | `ended_view`: `test_a_command_header_byte_is_refused` |
| C1.30 | `ended_view`: `test_a_section_of_a_broken_length_is_refused` |
| C1.31 | `ended_view`: `test_no_session_is_refused`; `ended`: `test_the_builder_refuses_no_session` |
| C1.32 | `ended_view`: `test_a_session_of_all_zero_is_refused_with_its_index` |
| C1.33 | `ended_view`: `test_sessions_that_do_not_ascend_are_refused` |
| C1.34 | `ended_view`: `test_the_section_length_is_checked_before_any_session` |
| C1.35 | `ended_view`: `test_one_session_and_two_read`; `ended`: `test_a_frame_holds_from_one_session_to_as_many_as_fit` |
| C1.36 | `ended_view`: `test_the_golden_ended_reads_and_builds_again`; `ended`: `test_an_ended_builds_the_golden_frame` |
| C1.37 | `declare_view`: `test_unused_bytes_are_refused` |
| C1.38 | `declare_view`: `test_a_kind_outside_its_set_is_refused` |
| C1.39, C1.40 | `declare_view`: `test_a_drop_that_does_not_fit_its_kind_is_refused` |
| C1.41, C1.42 | `declare_view`: `test_a_drop_names_no_built_on_and_no_commit_version` |
| C1.43 | `declare_view`: `test_a_commit_version_inside_an_epoch_is_refused` |
| C1.44 | `declare_view`: `test_the_header_fields_are_checked_in_order` |
| C1.45 | `declare_view`: `test_the_sections_are_checked_as_a_writes_before_the_records` |
| C1.46 | `declare_view`: `test_a_declare_of_no_declaration_is_refused`; `declare`: `test_the_builder_refuses_no_declaration` |
| C1.47 | `declare_view`: `test_a_record_that_is_no_declaration_is_refused_with_its_index`; kind 11 as *Found wanting*, 1 |
| C1.48 | `declare_view`: `test_one_declaration_reads` |
| C1.49, C1.53 | `declare_view`: `test_a_drop_holding_a_record_is_refused` |
| C1.50, C1.51 | `declare_view`: `test_a_drops_term_count_fits_its_kind`; `declare`: `test_the_builder_refuses_a_drop_whose_target_does_not_fit_its_kind` |
| C1.52 | `declare_view`: `test_a_drops_term_is_an_iri`; the triple term as *Found wanting*, 2 |
| C1.54 | `declare_view`: `test_a_reply_naming_one_version_alone_is_refused`; `declare`: `test_the_builder_refuses_a_reply_whose_versions_break_a_rule` |
| C1.55 | `declare_view`: `test_any_built_on_reads` |
| C1.56 | `declare_view`: `test_a_drops_reply_reads_with_or_without_a_version` |
| C1.57 | `declare_view`: `test_the_least_reply_reads` |
| C1.58 | `declare`: `test_declarations_filling_a_frame_are_built_and_read` |
| C1.59 | `declare_view`: `test_the_golden_declares_read_and_build_again`; `declare`: `test_a_declare_builds_the_golden_frames` |
| C1.60 | `defaults_view`: `test_unused_bytes_are_refused` |
| C1.61 | `defaults_view`: `test_the_term_table_is_checked_before_the_body` |
| C1.62 | `defaults_view`: `test_a_request_holding_an_entry_is_refused` |
| C1.63 | `defaults_view`: `test_a_reply_entry_that_is_no_iri_is_refused` |
| C1.64 | `defaults_view`: `test_an_entry_below_the_one_before_is_refused` |
| C1.65 | `defaults_view`: `test_an_iri_twice_in_one_frame_is_a_duplicate_term` |
| C1.66, C1.67 | `defaults_frames`: `test_the_order_runs_on_across_frames`, `test_an_entrys_index_runs_on_inside_a_later_frame` |
| C1.68 | `defaults_frames`: `test_a_frame_breaking_the_first_command_header_is_refused`, `test_a_frames_own_rules_come_before_the_first_frames_header` |
| C1.69 | `defaults_frames`: `test_no_frame_is_read_once_the_answer_passes_its_bound`, `test_the_default_bound_is_catalog_bytes_max` |
| C1.70, C1.71 | `defaults_view`: `test_a_reply_of_no_entry_and_of_one_reads` |
| C1.72 | `defaults_view`: `test_the_order_is_of_the_texts_utf8_bytes` |
| C1.73 | `defaults_frames`: `test_an_answer_in_one_frame_reads`; `defaults`: `test_an_answer_that_fits_is_one_frame` |
| C1.74 | `defaults_frames`: `test_an_answer_across_frames_reads_in_order`, `test_what_the_builder_cuts_reads_back`; `defaults`: `test_an_answer_past_max_frame_continues_in_whole_term_tables` |
| C1.75 | `defaults_frames`: `test_a_frame_of_no_entry_between_two_reads` |
| C1.76 | `defaults_view`: `test_the_golden_defaults_read_and_build_again`; `defaults`: `test_a_defaults_builds_the_golden_request_and_reply` |
| C1.77 | `challenge_view`: `test_the_unused_bytes_come_before_the_count` |
| C1.78 | `challenge_view`: `test_a_reply_counts_4096_to_ten_million_iterations`; `challenge`: `test_the_builder_refuses_what_the_view_refuses` |
| C1.79, C1.83 | `challenge_view`: `test_a_reply_reads_at_the_ends_of_its_ranges` |
| C1.80 | `challenge_view`: `test_the_name_is_utf8_and_checked_before_the_salt`, `test_a_name_ending_inside_a_character_is_refused` |
| C1.81 | `challenge_view`: `test_a_request_with_a_salt_of_one_byte_is_refused` |
| C1.82 | `challenge_view`: `test_a_request_holds_no_salt_and_a_reply_one` |
| C1.84 | `challenge_view`: `test_any_utf8_name_reads` |
| C1.85 | `challenge_view`: `test_the_golden_challenges_read_back_to_their_bodies`; `challenge`: `test_a_challenge_builds_the_golden_request_and_reply` |
| C1.86, C1.88 | `verify_view`: `test_the_header_is_checked_unused_bytes_verified_then_principal` |
| C1.87 | `verify_view`: `test_verified_is_a_boolean` |
| C1.89 | `verify_view`: `test_a_verified_reply_of_principal_zero_is_refused`; `verify`: `test_the_builder_refuses_a_verified_reply_of_principal_zero` |
| C1.90 | `verify_view`: `test_names_are_utf8_before_the_proof_is_counted`, `test_section_0_is_checked_before_section_1` |
| C1.91, C1.92, C1.93 | `verify_view`: `test_the_proof_and_the_signature_are_32_bytes_where_present`, `test_a_proof_of_no_byte_and_a_signature_of_33_are_refused` |
| C1.94 | `verify_view`: `test_the_proof_is_counted_before_the_signature` |
| C1.95 | `verify_view`: `test_a_request_and_an_unverified_reply_are_the_same_bytes` |
| C1.96 | `verify_view`: `test_the_golden_verifies_read_back_to_their_bodies`; `verify`: `test_a_verify_builds_the_golden_request_and_reply` |
| C1.97 | `subscribe_view`: `test_latest_is_a_boolean_that_names_no_epoch`, `test_unused_bytes_the_epoch_and_the_sections_are_checked` |
| C1.98 | `subscribe_view`: `test_a_subscribe_from_the_latest_close_reads`; `subscribe`: `test_a_subscribe_from_the_latest_close_is_laid_out` |
| C1.99 | `close_request_view`: `test_unused_header_bytes_are_refused`, `test_a_request_of_partition_zero_reads`, `test_an_epoch_of_none_is_refused_whatever_the_partition`; `close_request`: `test_the_frontends_request_names_partition_zero` |
| C1.100 | `catalog_view`: `test_a_request_names_an_epoch_only_without_a_version`; `catalog`: `test_a_request_at_an_epoch_is_laid_out` |
| C1.101 | `catalog_view`: `test_a_requests_epoch_is_within_42_bits` |
| The command table | `command`: `test_the_system_groups_commands_are_role_commands_and_537_and_540_are_unknown`, `test_exactly_the_registry_is_known`, `test_an_ended_is_answered_by_nothing` |

All 101 cases have a test. Two hold one input the specification's order makes unreachable, each tested at the refusal
the order gives: a record of kind 11 in C1.47 and a triple term in C1.52 (*Found wanting*, 1 and 2).

The role rule of a Subscribe's `latest`, which the scenario states and no body can see, is
`subscribe_view`: `test_only_a_frontends_connection_asks_from_the_latest_close`.

### The laws and their property tests

`tests/property/types/<view>_property_tests.rs`; expected bytes and refusals are laid out by hand in
`tests/property/utils/layout.rs`.

| Law | Property test |
|---|---|
| L1.1 | `session_view`: `test_a_session_round_trips` |
| L1.2 | `check_view`: `test_a_check_request_round_trips` |
| L1.3 | `check_view`: `test_a_check_reply_round_trips` |
| L1.4 | `ended_view`: `test_an_ended_round_trips` |
| L1.5, L1.6 | `declare_view`: `test_a_declare_of_declarations_round_trips`, a request or a reply by a drawn boolean |
| L1.7 | `declare_view`: `test_a_declare_of_a_drop_round_trips` |
| L1.8 | `defaults_view`: `test_a_defaults_round_trips` |
| L1.9 | `challenge_view`: `test_a_challenge_round_trips` |
| L1.10 | `verify_view`: `test_a_verify_round_trips` |
| L1.11 | `subscribe_view`: `test_a_subscribe_round_trips` |
| L1.12 | `close_request_view`: `test_a_close_request_round_trips` |
| L1.13 | `catalog_view`: `test_a_catalog_request_round_trips` |
| L1.14 | `session_view`: `test_a_session_breaking_one_rule_is_refused_with_that_rule` |
| L1.15 | `check_view`: `test_a_check_breaking_one_rule_is_refused_with_that_rule` |
| L1.16 | `ended_view`: `test_an_ended_breaking_one_rule_is_refused_with_that_rule` |
| L1.17 | `declare_view`: `test_a_declare_breaking_one_rule_is_refused_with_that_rule` |
| L1.18 | `defaults_view`: `test_a_defaults_breaking_one_rule_is_refused_with_that_rule` |
| L1.19 | `challenge_view`: `test_a_challenge_breaking_one_rule_is_refused_with_that_rule` |
| L1.20 | `verify_view`: `test_a_verify_breaking_one_rule_is_refused_with_that_rule` |
| L1.21 | `subscribe_view`: `test_a_subscribe_breaking_a_rule_is_refused_with_that_rule`; `close_request_view`: `test_a_close_request_breaking_a_rule_is_refused_with_that_rule`; `catalog_view`: `test_a_catalog_request_breaking_a_rule_is_refused_with_that_rule`, with case C1.101's epoch out of range |
| L1.22 | one per body, `test_a_<body>_breaking_two_rules_is_refused_with_the_earlier`: `session_view`, `check_view`, `ended_view`, `declare_view`, `defaults_view`, `challenge_view`, `verify_view` |
| L1.23 | one per body, `test_a_<body>_builder_refuses_what_its_view_refuses`: the same seven |
| L1.24 | `defaults_frames`: `test_a_defaults_answer_cut_at_any_frame_size_reads_back` |
| L1.25 | `check_view`: `test_a_reply_is_checked_against_its_request` |
| L1.26 | `command`: `test_a_number_is_a_command_exactly_when_the_table_assigns_it`, `test_the_system_groups_numbers_are_their_commands` |

All 26 laws have a property test, 40 tests in all. L1.5 draws predicates without a vector index: that record's layout
is the Write's, which the crate's tests of `WriteView` hold. L1.17 draws its term that is no IRI from kinds 2 to 4, for
the reason of *Found wanting*, 2.

### Every error variant reachable through these bodies

Each is asserted by its variant and fields, through a view or a builder: `ReservedNotZero`, `FieldValue`,
`CountMismatch`, `SectionLength` and `WrongCommand` by every body; `PaddingNotZero` (C1.12); `BodyTooShort` and
`SectionOffset` (C1.9); `TrailingBytes` (C1.4, C1.97); `NotUtf8` (C1.80, C1.90, and a term table's text through
Defaults); `DuplicateTerm` (C1.65); `BadTerm` and `CountMismatch` `"term text"` through a Defaults' term table;
`IndexOutOfRange` through a Declare's args; `FrameTooLarge` by the builders (C1.23, C1.35, C1.58) and by
`Defaults::frames`; `UnknownCommand` for 537 and 540; `CommandNotServed` for the five on a client connection. The group
adds no variant to `ProtocolError`.

### Phase 3: the suite's inputs against the enumeration

Reviewed for coincidence, case by case. What the review added, three tests:

- `defaults_view`: `test_the_term_tables_refusals_reach_through_the_body`: a term table's refusals other than the two
  C1.61 names reach through the body before the reply's rules.
- `declare_view`: `test_terms_and_names_no_record_names_are_read_past`: a `kind` 1 body holding a term and a `kind` 2
  body holding a name, which section 7.2 allows and no case named.
- `declare_view`: `test_the_records_refusals_reach_through_the_body`: section 4.6's rules of a record before the
  Declare's own.

What the review found already apart: every count refusal is asserted at two inputs whose `count` and `expected`
differ (2 against 3 and 3 against 2 for a Check's reply); a check's index at 0 and at 2; the bound of a Defaults
answer at a term table whose records and text differ in size (72 and 53 bytes), which tells the bytes counted, and
exactly at the bound and one past it; the fields of every golden body are distinct numbers (tenant 3, version 11 and
12, principal 9 and 11, position 1,250).

### Phase 4: the implementation

Judged by the suite alone: the first run of the implementation passed all of it, and no test was changed to pass. A
builder refuses through its decoder: `frame` lays the body out, hands the command header and the body to the view's
reader, the same function `from_frame` calls, and builds the frame only when it reads them
(`utils/checked_frame.rs`), so the two cannot differ.

| Target | Before the group | After |
|---|---|---|
| unit | 829 tests | 973 tests, 972 at the end of phase 4 and one more from phase 5 |
| property | none | 40 tests |
| bdd | 62 scenarios, 124 steps | 73 scenarios, 146 steps |

`ultragraphdb_protocol_transport`'s unit target went from 184 tests to 185 and `ultragraphdb_client_core`'s from 205
to 206, a test each for the arms of *Decided*, 1, beside two sweeps over `Command::ALL` brought from 20 role commands
to 25. The Clippy aspect is clean over the crate's library, unit tests, property tests and BDD target, with the
workspace table and with the lints task 1.1 added for its audit.

### Phase 5: fuzz targets and mutation testing

Five targets in `fuzz/ultragraphdb_protocol_wire/fuzz_targets/`, `session`, `check`, `ended`, `declare` and `defaults`:
each takes its input as a command header and a body and seals them into a frame, so every input reaches the body's
checks, then asserts what an accepted view promises and that it reads back to a body building the same frame (for
Declare, a frame that builds itself again). `body` takes the five views too. Seeds:
`testdata/fuzz_seeds/<target>/`, the golden frames from byte 128 on, and the nine golden frames whole under
`fuzz_seeds/body/`, written by `testdata/seeds.py`. `cargo +nightly fuzz build --fuzz-dir
fuzz/ultragraphdb_protocol_wire` builds all 17 targets. The runs of `scripts/fuzz.sh` are the stage's final
verification's.

Mutation testing, `scripts/mutants.sh <crate> --diff 5e7157ec`, each run to its end:

| Crate | Run | Mutants | Caught | Unviable | Missed |
|---|---|---|---|---|---|
| `ultragraphdb_protocol_wire` | 1, the diff | 349 | 294 | 49 | 6 |
| `ultragraphdb_protocol_wire` | 2, `--re 'in Defaults::frames'` | 8 | 8 | 0 | 0 |
| `ultragraphdb_protocol_transport` | 1 | 2 | 0 | 2 | 0 |
| `ultragraphdb_client_core` | 1 | 1 | 1 | 0 | 0 |

The six missed, each resolved:

- `Defaults::frames`, `flags | FLAG_LAST` with `^` and `flags & !FLAG_LAST` without `!`: a suite gap, every test gave a
  header of no flag. Killed by `defaults`: `test_frames_flags_the_last_frame_alone_under_a_header_flagged_last`; run 2
  catches both.
- Four equivalent, recorded in `.cargo/mutants.toml` with their reasons and confirmed by its `comm` check to match
  exactly these four: `CheckView::is_empty` with `false` (no view holds no check); `Check`'s and `Ended`'s
  `command_header` with zeroes (it is all zero); `Session`'s `body` with `vec![]` (the body of no section is empty).

No survivor is left open. The unit target runs 973 tests with the test of run 2.

## Found wanting

A weakness in a specification is fixed in the specification first. None of these is decided here; each is open,
routed to the user, with the reading the code holds until then. Items 1 and 2 of `notes/corner-cases.md` stay open as
they are: a Defaults answer past `catalog_bytes_max`, and the principal of a registration not yet durable.

1. **A record of kind 11 in a Declare (C1.47).** `notes/corner-cases.md`, C1.47: "a record of kind 7, of kind 11 and
   of kind 1 … `FieldValue` `"declaration"`". `PROTOCOL.md` §13.27, Order: "sections as section 7.2 states; for `kind`
   1 at least one record …, each of kind 8 to 10 (`FieldValue` `"declaration"`, the record's index)", and §4.6, step 1:
   "`FieldValue` (`"change kind"`): `kind` is 1 to 10 in a Write". A kind of 11 breaks section 7.2 first. Implemented:
   the Order line, `FieldValue` `"change kind"` for 11 and `"declaration"` for kinds 1 to 7.
2. **A triple term as a Drop's one term (C1.52).** C1.52: "`drop` 1 whose term is a blank node, a literal, a triple
   term: `FieldValue` `"drop term"`". §4.4: "A triple term names three entries before its own", so a table holding
   one holds four entries, and §13.27's Order has the count first (`CountMismatch` `"drop terms"`). §6.5's Drop has
   the same unreachable case. Implemented: the count's refusal.
3. **The value a `FieldValue` carries, where the text names the field alone.** §13.23 (`"session"`), §13.27
   (`"version"` in a `kind` 1 reply naming one of `version` and `commit_version`), §13.3 (`"latest"` with a `from`).
   Implemented: 0 for `"session"`; the `version` field's value for `"version"`; the `latest` byte for `"latest"`.
4. **The role half of "A Subscribe from the latest close names no epoch".** wire-protocol, that scenario: "`latest` 1 on
   a connection whose role is `storage` … refused with `FieldValue` naming `latest`", against §13.3 as amended in
   *Decided*, 5: the role is the receiving session's to check. A body's decoder cannot bind the scenario's second half.
   Implemented: `SubscribeView::check_sender(RoleKind)`, the rule stated once in this crate for the session to call, as
   `DefaultEntryView::check_request` is; `CloseRequestView` holds no such method, since no scenario of this group
   needs one. Whether the wire crate is where the rule's text lives is the user's to confirm before group 3 calls it.
5. **A Defaults answer past `catalog_bytes_max`, what the code does (item 1 of `notes/corner-cases.md`, open).**
   §13.28 names no refusal and no bytes. Implemented: `DefaultsFrames` counts sections 0 and 1 of every frame taken;
   the frame that takes them past the bound is not read and no frame after it, `push` answers `Ok`, and
   `is_past_bound` says so. No refusal is invented.
6. **A first Defaults or Catalog frame of `position` 0.** §13.28 and §13.7: a body of `position` 0 is a request; neither
   says whether a requester refuses one arriving as the first frame of an answer. `CatalogFrames` takes it, and
   `DefaultsFrames` does as it does.
