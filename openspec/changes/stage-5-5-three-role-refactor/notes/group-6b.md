# Group 6b: the placement model

`lean/PlacementModel`, a Lake library beside `RoundModel` in `lean/lakefile.lean`, both default targets. `cd lean &&
lake build` (or `lake build PlacementModel`) is green from a clean build in about 4 s, with no `sorry`, no `admit`, no
`native_decide` and no `axiom` declaration; `#print axioms` on every theorem below names only `propext`, `Quot.sound`
and `Classical.choice`, as RoundModel's do.

**Outcome.** Rules 4 and 11 hold for P24 as written. Rules 5, 6 and 9 do **not** hold for P24's reclamation as written:
two concrete runs, checked in Lean, end with an edge visible at a closed epoch that a reverse read misses. They hold,
proved, for P24 with three added rules (`Protocol.repaired`). Which rules to adopt is the user's decision; see
*Findings*.

| File | Holds |
|---|---|
| `lean/PlacementModel.lean` | The root and its module table |
| `lean/PlacementModel/Basic.lean` | Edge and member events, the latest event, visibility at an epoch |
| `lean/PlacementModel/State.lean` | `World`, `Protocol` (design and repaired), `State`, messages, the 13 steps |
| `lean/PlacementModel/Base.lean` | `BaseInv`, kept by every protocol variant |
| `lean/PlacementModel/Repaired.lean` | `RepInv`, kept by the repaired protocol |
| `lean/PlacementModel/Theorems.lean` | The rules' theorems, each with its simulator invariant |
| `lean/PlacementModel/Counterexamples.lean` | The two refuting runs and the specification's stale-report run |

## The model

- **World** (what the metadata group decides): `grp` (a subject's bucket group), `place` (the partition holding a
  subject's bucket at each epoch — the placement map, changing by epoch, at bucket granularity, so a group may span
  partitions), `home` (an entity's home).
- **State**: the sequencer's latest close `seq`, pending catalog versions, assigned `(version, E)` pairs in the close
  records, each partition's latest delivered close and held versions, every edge event (subject, entity, stamp, writer,
  insert/delete) and every member event of every `R(o)` (entity, group, epoch, generation, add/remove), each
  partition's marks `(gen, regEpoch)`, last seen generations and mark-key versions, every message, a record of every
  commit (partition, stamp, held versions, registrations referenced), tombstones.
- **Steps** (each atomic and durable): `propose` (pending version), `close` (close `c + 1` assigns `E = c + 2` to every
  pending version), `deliver` (next close, in order, with its announced versions), `register`, `accept` (deleted after a
  tombstone; a repeat answers the current generation; otherwise add/renew at the home's open epoch, reply with its
  latest delivered close `c`, event and reply in one durable step), `insertHome` (the home's own commit adds its group if
  absent), `insertMark`, `insertReply` (needs `c ≤` its delivered close and an unchanged mark key; writes the mark),
  `delete`, `report` (no edges from its subjects of `g`, clears the mark, sends `(gen, epoch)`), `reclaim` (removes the
  member as an MVCC write at the home's open epoch iff the generation is current), `tombstone`, `crash`.
- Every commit is stamped `delivered + 1`. A read at `e` sees each key's latest event stamped `≤ e`;
  `stamps_never_fall_along_a_key` proves that is MVCC's highest version.
- **Messages** only accumulate and any step may act on any message, so every message can be delayed, reordered,
  duplicated or lost. Because every step is durable and a transaction's volatile state is its commit step's guard,
  `crash` is the identity on the state.
- **Reverse read** (`reverseRead`, `ReadReady`): `R(o)` at `e` at the home once it has closed `e`, each member group's
  subjects answered from the partition the map gives their bucket at `e` once it has closed `e`.

## Theorems

| Requirement / scenario | Theorem | Protocol |
|---|---|---|
| "The placement model builds without gaps" | `lake build PlacementModel` green, no `sorry` (checked by the build) | — |
| "A stale reclamation report removes nothing" | `a_stale_reclamation_report_removes_nothing` (any state: the report changes nothing; repaired: every edge covered), `a_stale_reclamation_report_removes_nothing_trace` (the scenario's run: generation 3 reported, re-registered to 4, report arrives, member kept, new edge covered; a later report of 4 removes it) | any / repaired |
| Rule 4, the epoch inequality, every write path | `epoch_inequality`, `stamps_above_delivered_close` | any, P24 as written included |
| Rules 5 and 6 with 4, reverse reads | `reverse_read_complete`: a read answered at closed `e` in state `σ` returns every edge any later state shows at `e` | repaired |
| Rule 9, reclamation | `reclamation_keeps_every_member_with_an_edge` | repaired |
| Rule 11, catalog activation | `no_stamp_before_its_catalog_version`, `partition_holds_versions_of_its_open_epoch`, `close_assigns_one_above` | any, P24 as written included |
| Tombstones | `registration_after_tombstone_adds_nothing` | any |
| Refutations | `removal_below_the_report_epoch_loses_an_edge`, `split_group_report_loses_an_edge` | repaired minus one rule |

## Findings needing the user

### 1. The home's removal can land below the report's epoch (counterexample, checked in Lean)

P24 rule 9 has the home remove the member "as an MVCC write", at its own open epoch, with no relation to the report's
epoch. A home whose delivered closes lag the reporter's removes the member at an epoch where the reporter's edge is
still visible. Trace (`Counterexample1`): partition 0 homes entity 5; partition 1 holds subject 0 (group 0).

1. Closes 1–6 issued; partition 1 receives 1–5 (open epoch 6); the home receives none (open epoch 1).
2. P1 registers `(5, g0)` naming generation 0. The home adds g0 at **epoch 1**, generation 1, replies close 0.
3. P1 commits edge `0 → 5` at epoch 6 under the reply (0 ≤ 5).
4. P1 receives close 6; at epoch 7 it deletes the edge, then reports "no edges as of 7, generation 1".
5. The home, still at open epoch 1, applies it: generation 1 is current, so it removes g0 **at epoch 1**.
6. The home receives closes 1–6. A reverse read of 5 at closed epoch 6 finds `R(5)` empty and misses `0 → 5`, which is
   visible at 6.

**Repair** (`removalAwaitsReportEpoch`): the home applies a report only once its open epoch is at or above the report's
epoch, i.e. once close `r − 1` has been delivered to it — rule 4's shape applied to removals.

### 2. A bucket group spanning partitions shares one generation (counterexample, checked in Lean)

A member and its generation are per `(o, g)`, but a group's buckets can be held by several partitions: the initial map
splits 65,536 buckets at non-multiples of 64 (the metadata-group scenario's range 21,846..43,691), the partitions spec
resolves a group to "the partitions holding its buckets", and a move leaves the old holder with a mark. Trace
(`Counterexample2`): partition 0 homes entity 5; group 0 has subject 0 on partition 1 and subject 1 on partition 2.

1. P1 registers `(5, g0)` naming 0; the home adds g0 at generation 1.
2. P2 registers `(5, g0)` naming 0: older than current, a repeat, answered generation 1.
3. P1 commits `0 → 5`, P2 commits `1 → 5`, both marked generation 1.
4. P1 deletes `0 → 5` and reports "no edges from my subjects of g0, generation 1".
5. The home removes g0 (generation unchanged). P2's edge `1 → 5` is visible; `R(5)` no longer names g0; a reverse read
   misses it.

The same happens after a move: the old holder has no subjects of the group left, reports vacuously with the current
generation, and the member goes while the new holder's edges remain.

**Repair proved** (`reportNeedsWholeGroup`): only a partition holding every bucket of the group at every epoch may report
for it; a split or ever-moved group is never reclaimed. This is the narrowest rule that proves. A design that reclaims
split groups needs generations or membership per (group, registering partition) — the wire's `ReferenceRegistration`
already names the registrant's partition — and a move protocol that carries marks; that is a design decision for the
user, and the model would then need re-proving.

### 3. Generations after a removal (no Lean refutation; reason for the third rule)

Two gaps in "one generation above the one named":

- A registration naming a generation **above** the member's current one is undefined (P24 lists only current, older,
  not-a-member). It arises once a removed member is re-added at a reused generation. If it is answered as a repeat,
  this run loses an edge: P1 holds the mark at generation 2 and reports 2 (removed); re-registers naming 2, re-added at
  3; reports 3 (removed); a duplicated old registration naming 1 re-adds the member at 2; P1 registers naming 3 —
  "above current" — gets the repeat answer 2 and commits an edge under mark 2; the duplicated old report of 2 arrives,
  matches, and removes the member under the edge.
- Re-adding an absent member at "one above the named" reuses generations an old duplicated report may carry.

**Repair proved** (`generationsOnlyRise`): a member is added or renewed at `max(named, highest it ever had) + 1`. It
agrees with every partitions-spec scenario. It requires the home to remember a removed member's last generation for as
long as a duplicate registration or report can arrive — physical deletion of the removal record must also wait out the
message lifetime, not only the history-retention horizon. Whether P24's literal rule with "above current" treated as a
renewal is also safe is plausible and not proved.

### Assumptions the model makes (stated, each needing the user's confirmation)

1. **A commit under a reply validates the mark key.** A transaction that read its mark absent and registered aborts if a
   commit wrote that mark key since (exact validation with the marks key in its read set). Without it: T0 and T1 both
   register at generation 1; T0 commits and marks; T2 deletes the edge and reports, clearing the mark; T1 commits under
   its old reply and re-marks at generation 1; the report matches and removes the member under T1's edge. Group 7 must
   put the marks key in the read set; the design does not say so.
2. **The sequenced path (stage 8) is `insertReply`:** applied at its partition's open epoch with `c + 1 ≤ stamp`, and
   checking the mark key locally as in assumption 1. P24 says the blind path has no validation *exchange*; if it also
   skips the local mark check, assumption 1's race reopens.
3. **A partition's latest delivered close survives its crash** (in its log, or it stamps nothing until the sequencer
   re-delivers at least that close). Otherwise a restarted partition could stamp an epoch a read already closed.
4. **Moves hand over** (`Handover`): a partition writes or answers for a moved-in bucket only once every earlier holder
   has closed the epochs it held it, and the bucket's data follows it. Stage 5.5 builds no moves; whoever specifies them
   needs this rule.
5. **Homes do not move**, and the home's own groups are never reclaimed (the home keeps no mark, and P24 does not say how
   it reclaims them).
6. **Retention is infinite**: physical deletion below the horizon and the IRI-to-ID entry's reclamation are not modelled
   (they remove only what no read can name).
7. Predicate and graph IRI registrations, which join no referrer set, are not modelled; each commit writes one edge event
   (a multi-write transaction is a run of steps in one epoch, which no closed-epoch read can split).

## 6b.4: invariants group 7's simulator checks after every step

In terms of the partition logs, referrer sets (`R(o)` at each home), marks keyspaces and delivered closes group 7
builds. "Visible at `e`" means the latest version at or below epoch `e`.

1. **Stamping** (`stamps_above_delivered_close`): every commit a partition appends, including a registration it admits
   as home and a removal, carries epoch `last delivered close + 1`; no commit carries an epoch whose close the partition
   already held.
2. **Epoch inequality** (`epoch_inequality`, rule 4): for every commit and every registration `(o, g, a)` it references
   — through a fresh reply (`a = c + 1`), its mark (`a` = the mark's registration epoch) or the home's own add — `a ≤`
   the commit's epoch, and `R(o)` at the home has an add or renew of `g` at an epoch `≤ a`.
3. **Catalog activation** (`no_stamp_before_its_catalog_version`, rule 11): for every commit at epoch `t` and every
   catalog version whose effective epoch in the sequencer's close records is `≤ t`, the partition had loaded that
   version when it committed; every partition holds every version effective at or below its open epoch.
4. **Ordering** (`stamps_never_fall_along_a_key`): along each partition log, and across a bucket's moves, a subject's
   edge commits carry non-decreasing epochs; along each home's log, an entity's referrer-set writes carry
   non-decreasing epochs.
5. **Coverage** (`reclamation_keeps_every_member_with_an_edge`, rule 9): for every edge `s → o` some partition's local
   reverse index holds at an epoch `e` (every retained `e` up to the open one), `R(o)` at `o`'s home holds `s`'s bucket
   group at `e`.
6. **Reverse-read completeness** (`reverse_read_complete`, rules 5 and 6): every reverse read of `o` at a closed epoch
   `e` answered at step `i` contains, at every later step `j`, every subject whose edge to `o` the single-partition
   reference model holds at `e` after step `j`.
7. **Stale reports** (`a_stale_reclamation_report_removes_nothing`): a report whose generation differs from the member's
   current one leaves the home's referrer set and log unchanged.
8. **Supporting checks** (the fields of `BaseInv` and `RepInv`, useful as early alarms): a mark rests on an add/renew at
   or below its registration epoch and that epoch is `≤` its holder's open epoch; a mark's generation is one the member
   has had; a reply's close `c` is at or below the home's delivered close; while a member stands at a report's
   generation, the reporter holds no mark, has seen that generation, and no subject of the group has an edge to the
   entity at or after the report's epoch.

With the design as written, checks 5 and 6 fail on the two traces of findings 1 and 2; those traces are ready-made
simulator scenarios.
