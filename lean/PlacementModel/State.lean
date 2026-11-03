/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import PlacementModel.Basic

/-!
# The cluster: partitions, the sequencer, registrations, reclamation and their steps

The `World` fixes what the metadata group decides: each subject's bucket group, the placement map, which gives every
subject's partition at every epoch and so changes by epoch, each placement version's layout, and each entity's home.
The `State` holds everything the cluster stores durably: the sequencer's closes with the catalog and placement versions
they announce, each partition's latest delivered close and the catalog and placement versions it holds, every edge
event and every member event, each partition's marks, last seen generations and mark versions, every message ever sent,
a record of every commit, and the tombstones.

A `Step` is one atomic, durable action of one participant:

* `propose` — the metadata group commits a catalog version as pending.
* `proposePlacement` — the metadata group commits a placement version as pending; no epoch is named. The initial even
  map is placement version 0, in effect from epoch 0.
* `close` — the sequencer closes epoch `c + 1` and assigns every pending catalog version, and the oldest pending
  placement version if any, the effective epoch `E = c + 2` in that close's record, which announces it (P24 rule 11).
  Two placement versions never share an effective epoch, so the rest wait for later closes.
* `deliver` — a partition receives the next close, in order, with the catalog and placement versions it announces.
* `register` — a partition sends an entity's home a registration for a bucket group, naming the generation it last saw
  and the version of its mark key at the time.
* `accept` — the home answers a registration: "deleted" after a tombstone; for a repeat, the current generation;
  otherwise it adds or renews the member one generation above, at its open epoch, and replies with its latest delivered
  close `c`. The member event and the reply are one durable step: the reply leaves only once the event is durable.
* `insertHome`, `insertMark`, `insertReply` — a partition commits an edge from one of its subjects, stamped at its open
  epoch, the latest delivered close plus one, recording the catalog and placement versions it holds. An edge to an entity it homes adds its own group to `R(o)` in the same
  commit if it is not a member; an edge to another partition's entity needs its mark, or a reply whose close has been
  delivered to it and whose mark key is unchanged since the registration read it, and writes the mark.
* `delete` — a partition commits the deletion of an edge from one of its subjects.
* `report` — a partition with no edges to `o` from its subjects of group `g` clears its mark in a commit that sends the
  home a reclamation report with the mark's generation and the commit's epoch (P24 rule 9).
* `reclaim` — the home applies a report: it removes the member, as an MVCC write at its open epoch, only if the
  member's current generation is the one reported.
* `tombstone` — the home writes an entity's tombstone; later registrations are answered "deleted".
* `crash` — a participant loses power and restarts.

**Messages.** `msgs` only grows, and a step may act on any message in it, so every message can be delayed, reordered,
delivered any number of times, or never (lost). A crash therefore loses nothing the model stores: every step is
durable when it completes, a transaction's volatile state is the guard of the one commit step that uses it, and a reply
whose transaction died with the crash is a message nothing acts on again. `crash` is the identity on the state.

**Moves.** A partition writes a subject only at an epoch the map gives it the subject's bucket, and only once every
earlier holder of the bucket has closed the epochs it held it (`Handover`): the hand-over a move completes before the
new holder writes or answers for the bucket. The data of a bucket follows it, so the partition the map names at an
epoch answers for the bucket's whole history.

**Placement versions.** The map `World.place` stays an oracle by epoch, and the steps keep reading it. `Placed` ties it
to the versioned placement: at every epoch the sequencer has announced, at or below `seq + 1`, it is the layout of the
placement version in effect there (`State.versionAt`). `PlacementModel.Theorems` proves the version in effect at such an
epoch never changes later, that a run agreeing with `Placed` at its end agreed at every earlier state, and that a
partition routes at its open epoch through a version it holds. `Handover` stays a premise of the write steps: proving it
needs the hand-over protocol, which the model does not contain.

**The protocol flags.** `Protocol.design` is P24 as written. `Protocol.repaired` adds the three rules the proofs need,
each refuted without it in `PlacementModel.Counterexamples` or argued in the notes: the home removes a member only at an
epoch at or above the report's (`removalAwaitsReportEpoch`); only a partition holding every bucket of the group at
every epoch may report for it (`reportNeedsWholeGroup`); and a member's new generation is above every generation it has
had (`generationsOnlyRise`).

**Simplifications.** Entity homes do not move. Each commit writes one edge event; a transaction writing several, or
deleting an edge and reporting in one commit, is a run of steps in one epoch that a closed-epoch read cannot split.
Predicate and graph IRIs, which join no referrer set, are not modelled. The sequenced path of stage 8 is a commit at the
partition's open epoch with the guard `c + 1 ≤ stamp` for its registrations, which is `insertReply`'s.
-/

namespace PlacementModel

/-- What the metadata group decides: each subject's bucket group, the placement map by epoch, and each entity's home. -/
structure World where
  /-- The bucket group of a subject: its bucket's top 18 bits. -/
  grp : Nat → Nat
  /-- The partition holding a subject's bucket at an epoch. -/
  place : Nat → Nat → Nat
  /-- The partition holding a subject's bucket under a placement version: version 0 is the initial even map. -/
  layout : Nat → Nat → Nat
  /-- An entity's home partition. -/
  home : Nat → Nat

/-- The variant of the protocol a run follows. -/
structure Protocol where
  /-- The home removes a member only once its open epoch is at or above the report's epoch. -/
  removalAwaitsReportEpoch : Bool
  /-- Only a partition holding every bucket of a group, at every epoch, reports for it. -/
  reportNeedsWholeGroup : Bool
  /-- A member's new generation is one above the higher of the generation named and every generation it has had. -/
  generationsOnlyRise : Bool

/-- P24 as written. -/
def Protocol.design : Protocol := ⟨false, false, false⟩

/-- P24 with the three rules the reclamation proofs need. -/
def Protocol.repaired : Protocol := ⟨true, true, true⟩

/-- Partition `p` holds every bucket of group `g` at every epoch. -/
def Whole (W : World) (g p : Nat) : Prop := ∀ t s, W.grp s = g → W.place t s = p

/-- Partition `p` holds some bucket of group `g` at some epoch. -/
def CanHold (W : World) (g p : Nat) : Prop := ∃ s t, W.grp s = g ∧ W.place t s = p

/-- Every partition that held `s`'s bucket at an epoch below `t` has closed that epoch. -/
def Handover (W : World) (deliv : Nat → Nat) (s t : Nat) : Prop := ∀ t', t' < t → t' ≤ deliv (W.place t' s)

/-- A partition's mark of `(o, g)`: the generation it holds and the epoch of the registration it rests on. -/
structure Mark where
  gen : Nat
  regEpoch : Nat
  deriving DecidableEq, Repr

/-- A message between partitions. -/
inductive Msg where
  /-- A registration from `src` for `(o, g)`, naming generation `n`, sent when its mark key had version `mv`. -/
  | reg (src o g n mv : Nat)
  /-- The home's reply to `dst` for `(o, g)`: generation `gen`, its latest delivered close `c`, the registration's
  `mv`, and whether the entity was deleted. -/
  | reply (dst o g gen c mv : Nat) (deleted : Bool)
  /-- A reclamation report from `src`: no edges to `o` from its subjects of `g` as of epoch `r`, at generation `k`. -/
  | report (src o g k r : Nat)
  deriving DecidableEq, Repr

/-- A reference a commit makes to a registration: the entity, the group and the registration's epoch. -/
structure Ref where
  obj : Nat
  grp : Nat
  epoch : Nat
  deriving DecidableEq, Repr

/-- A commit at a partition: its stamp, the catalog and placement versions the partition held, and the registrations it
references. -/
structure CommitRec where
  part : Nat
  stamp : Nat
  held : List Nat
  pheld : List Nat
  refs : List Ref
  deriving DecidableEq, Repr

/-- Everything the cluster stores. -/
structure State where
  /-- The sequencer's latest close: closes `1 … seq` are issued. -/
  seq : Nat
  /-- Catalog versions committed and not yet assigned an epoch. -/
  pending : List Nat
  /-- Every version assigned, with its effective epoch, in the close records. -/
  assigned : List (Nat × Nat)
  /-- Each partition's latest delivered close. -/
  delivered : Nat → Nat
  /-- The catalog versions each partition holds. -/
  held : Nat → List Nat
  /-- Placement versions committed and not yet assigned an epoch, oldest first. -/
  ppending : List Nat
  /-- Every placement version assigned, with its effective epoch, in the close records; version 0 from epoch 0. -/
  passigned : List (Nat × Nat)
  /-- The placement versions each partition holds. -/
  pheld : Nat → List Nat
  /-- Every edge event, in commit order. -/
  edges : List EdgeEv
  /-- Every member event of every referrer set, in commit order. -/
  mems : List MemEv
  /-- Each partition's mark of each `(o, g)`. -/
  marks : Nat → Nat → Nat → Option Mark
  /-- The generation each partition last saw for each `(o, g)`. -/
  seen : Nat → Nat → Nat → Nat
  /-- The version of each partition's mark key of each `(o, g)`: how often a commit wrote it. -/
  mver : Nat → Nat → Nat → Nat
  /-- Every message sent. -/
  msgs : List Msg
  /-- Every commit, at any partition, in commit order. -/
  commits : List CommitRec
  /-- Tombstones: the entity and the epoch of its deletion. -/
  tombs : List (Nat × Nat)

/-- The cluster before any step: close 0 counts as delivered everywhere, so every partition's open epoch is 1; the
initial even map, placement version 0, is in effect from epoch 0 and held everywhere. -/
def State.init : State :=
  { seq := 0, pending := [], assigned := [], delivered := fun _ => 0, held := fun _ => [], ppending := [],
    passigned := [(0, 0)], pheld := fun _ => [0], edges := [], mems := [], marks := fun _ _ _ => none,
    seen := fun _ _ _ => 0, mver := fun _ _ _ => 0, msgs := [], commits := [], tombs := [] }

/-- `f` with `p` set to `v`. -/
def upd1 {β : Type} (f : Nat → β) (p : Nat) (v : β) : Nat → β := fun p' => if p' = p then v else f p'

/-- `f` with `(p, o, g)` set to `v`. -/
def upd3 {β : Type} (f : Nat → Nat → Nat → β) (p o g : Nat) (v : β) : Nat → Nat → Nat → β :=
  fun p' o' g' => if p' = p ∧ o' = o ∧ g' = g then v else f p' o' g'

namespace State

variable (σ : State)

/-- A partition's open epoch: one above its latest delivered close. It stamps every commit with it. -/
def openEpoch (p : Nat) : Nat := σ.delivered p + 1

/-- A commit record at `p`'s open epoch with the catalog and placement versions `p` holds. -/
def record (p : Nat) (refs : List Ref) : List CommitRec :=
  σ.commits ++ [⟨p, σ.openEpoch p, σ.held p, σ.pheld p, refs⟩]

/-- The placement version in effect at epoch `t`: the one assigned last at or below `t`, the initial map if none. -/
def versionAt (t : Nat) : Nat :=
  match lastWhere σ.passigned (fun x => decide (x.2 ≤ t)) with
  | some x => x.1
  | none => 0

/-- The entity has a tombstone. -/
def tombstoned (o : Nat) : Bool := σ.tombs.any (fun x => decide (x.1 = o))

/-- Group `g` is a member of `R(o)` in the latest state. -/
def isMember (o g : Nat) : Bool :=
  match memCur σ.mems o g with
  | some ev => ev.present
  | none => false

/-- The current generation of member `g` of `R(o)`. -/
def curGen (o g : Nat) : Nat :=
  match memCur σ.mems o g with
  | some ev => ev.gen
  | none => 0

/-- A registration naming `n` is a repeat: the member is present at a higher generation. -/
def isRepeat (o g n : Nat) : Bool :=
  match memCur σ.mems o g with
  | some ev => ev.present && decide (n < ev.gen)
  | none => false

/-- Member `g` of `R(o)` is present at generation `k`. -/
def curIs (o g k : Nat) : Bool :=
  match memCur σ.mems o g with
  | some ev => ev.present && decide (ev.gen = k)
  | none => false

/-- The generation a registration naming `n` adds or renews a member at. -/
def nextGen (P : Protocol) (o g n : Nat) : Nat :=
  if P.generationsOnlyRise then max n (genMax σ.mems o g) + 1 else n + 1

/-- The metadata group commits a catalog version as pending. -/
def propose (v : Nat) : State := { σ with pending := σ.pending ++ [v] }

/-- The metadata group commits a placement version as pending. -/
def proposePlacement (v : Nat) : State := { σ with ppending := σ.ppending ++ [v] }

/-- The sequencer closes epoch `seq + 1`, assigning every pending catalog version, and the oldest pending placement
version if any, the effective epoch `seq + 2`. -/
def close : State :=
  { σ with seq := σ.seq + 1, assigned := σ.assigned ++ σ.pending.map (fun v => (v, σ.seq + 2)), pending := [],
           passigned := σ.passigned ++ (σ.ppending.take 1).map (fun v => (v, σ.seq + 2)),
           ppending := σ.ppending.drop 1 }

/-- `p` receives close `delivered p + 1` and the catalog and placement versions it announces, those taking effect at
`delivered p + 2`. -/
def deliver (p : Nat) : State :=
  { σ with
    delivered := upd1 σ.delivered p (σ.delivered p + 1)
    held := upd1 σ.held p (σ.held p ++ (σ.assigned.filter (fun x => decide (x.2 = σ.delivered p + 2))).map Prod.fst)
    pheld := upd1 σ.pheld p
      (σ.pheld p ++ (σ.passigned.filter (fun x => decide (x.2 = σ.delivered p + 2))).map Prod.fst) }

/-- `p` sends `o`'s home a registration for group `g`. -/
def register (p o g : Nat) : State := { σ with msgs := σ.msgs ++ [.reg p o g (σ.seen p o g) (σ.mver p o g)] }

/-- The home adds or renews member `g` of `R(o)` at its open epoch and replies. -/
def addMember (W : World) (P : Protocol) (src o g n mv : Nat) : State :=
  { σ with
    mems := σ.mems ++ [⟨o, g, σ.openEpoch (W.home o), σ.nextGen P o g n, true⟩]
    msgs := σ.msgs ++ [.reply src o g (σ.nextGen P o g n) (σ.delivered (W.home o)) mv false]
    commits := σ.record (W.home o) [] }

/-- The home answers a registration from `src` for `(o, g)` naming `n`. -/
def accept (W : World) (P : Protocol) (src o g n mv : Nat) : State :=
  if σ.tombstoned o then { σ with msgs := σ.msgs ++ [.reply src o g 0 (σ.delivered (W.home o)) mv true] }
  else if σ.isRepeat o g n then
    { σ with msgs := σ.msgs ++ [.reply src o g (σ.curGen o g) (σ.delivered (W.home o)) mv false] }
  else σ.addMember W P src o g n mv

/-- The home `p` commits an edge from its subject `s` to `o`, adding `s`'s group to `R(o)` if it is not a member. -/
def insertHome (W : World) (p s o : Nat) : State :=
  { σ with
    edges := σ.edges ++ [⟨s, o, σ.openEpoch p, p, true⟩]
    mems := if σ.isMember o (W.grp s) then σ.mems
      else σ.mems ++ [⟨o, W.grp s, σ.openEpoch p, genMax σ.mems o (W.grp s) + 1, true⟩]
    commits := σ.record p [⟨o, W.grp s, σ.openEpoch p⟩] }

/-- `p` commits an edge from `s` to `o` under its existing mark. -/
def insertMark (W : World) (p s o : Nat) (mk : Mark) : State :=
  { σ with
    edges := σ.edges ++ [⟨s, o, σ.openEpoch p, p, true⟩]
    commits := σ.record p [⟨o, W.grp s, mk.regEpoch⟩] }

/-- `p` commits an edge from `s` to `o` under a reply naming generation `gen` and close `c`, writing the mark. -/
def insertReply (W : World) (p s o gen c : Nat) : State :=
  { σ with
    edges := σ.edges ++ [⟨s, o, σ.openEpoch p, p, true⟩]
    marks := upd3 σ.marks p o (W.grp s) (some ⟨gen, c + 1⟩)
    seen := upd3 σ.seen p o (W.grp s) gen
    mver := upd3 σ.mver p o (W.grp s) (σ.mver p o (W.grp s) + 1)
    commits := σ.record p [⟨o, W.grp s, c + 1⟩] }

/-- `p` commits the deletion of the edge from `s` to `o`. -/
def delete (p s o : Nat) : State :=
  { σ with edges := σ.edges ++ [⟨s, o, σ.openEpoch p, p, false⟩], commits := σ.record p [] }

/-- `p` clears its mark of `(o, g)` and reports, in the same commit, no edges as of its open epoch. -/
def report (p o g : Nat) (mk : Mark) : State :=
  { σ with
    marks := upd3 σ.marks p o g none
    seen := upd3 σ.seen p o g mk.gen
    mver := upd3 σ.mver p o g (σ.mver p o g + 1)
    msgs := σ.msgs ++ [.report p o g mk.gen (σ.openEpoch p)]
    commits := σ.record p [] }

/-- The home applies a report naming generation `k` and epoch `r`: it removes the member only if its generation is
unchanged, and, in the repaired protocol, only once its open epoch has reached `r`. Otherwise nothing changes; the
report stays in `msgs` and can be applied later. -/
def onReport (W : World) (P : Protocol) (o g k r : Nat) : State :=
  if σ.curIs o g k && (!P.removalAwaitsReportEpoch || decide (r ≤ σ.openEpoch (W.home o))) then
    { σ with
      mems := σ.mems ++ [⟨o, g, σ.openEpoch (W.home o), k, false⟩]
      commits := σ.record (W.home o) [] }
  else σ

/-- The home writes `o`'s tombstone at its open epoch. -/
def tombstone (W : World) (o : Nat) : State :=
  { σ with tombs := σ.tombs ++ [(o, σ.openEpoch (W.home o))], commits := σ.record (W.home o) [] }

end State

/-- The world's placement map is the versioned one at every epoch the sequencer has fixed: at each epoch at or below
`seq + 1`, every subject's partition is the one the placement version in effect there gives it. -/
def Placed (W : World) (σ : State) : Prop := ∀ t, t ≤ σ.seq + 1 → ∀ s, W.place t s = W.layout (σ.versionAt t) s

/-- One atomic, durable step of one participant. -/
inductive Step (W : World) (P : Protocol) : State → State → Prop where
  | propose (σ : State) (v : Nat) : Step W P σ (σ.propose v)
  | proposePlacement (σ : State) (v : Nat) : Step W P σ (σ.proposePlacement v)
  | close (σ : State) : Step W P σ σ.close
  | deliver (σ : State) (p : Nat) (h : σ.delivered p < σ.seq) : Step W P σ (σ.deliver p)
  | register (σ : State) (p o g : Nat) (h : p ≠ W.home o) : Step W P σ (σ.register p o g)
  | accept (σ : State) (src o g n mv : Nat) (h : Msg.reg src o g n mv ∈ σ.msgs) :
      Step W P σ (σ.accept W P src o g n mv)
  | insertHome (σ : State) (p s o : Nat) (hp : W.place (σ.openEpoch p) s = p) (hh : W.home o = p)
      (hand : Handover W σ.delivered s (σ.openEpoch p)) : Step W P σ (σ.insertHome W p s o)
  | insertMark (σ : State) (p s o : Nat) (mk : Mark) (hp : W.place (σ.openEpoch p) s = p) (hh : W.home o ≠ p)
      (hm : σ.marks p o (W.grp s) = some mk) (hand : Handover W σ.delivered s (σ.openEpoch p)) :
      Step W P σ (σ.insertMark W p s o mk)
  | insertReply (σ : State) (p s o gen c mv : Nat) (hp : W.place (σ.openEpoch p) s = p) (hh : W.home o ≠ p)
      (hr : Msg.reply p o (W.grp s) gen c mv false ∈ σ.msgs) (hv : mv = σ.mver p o (W.grp s))
      (hc : c ≤ σ.delivered p) (hand : Handover W σ.delivered s (σ.openEpoch p)) :
      Step W P σ (σ.insertReply W p s o gen c)
  | delete (σ : State) (p s o : Nat) (hp : W.place (σ.openEpoch p) s = p)
      (hand : Handover W σ.delivered s (σ.openEpoch p)) : Step W P σ (σ.delete p s o)
  | report (σ : State) (p o g : Nat) (mk : Mark) (hm : σ.marks p o g = some mk)
      (hno : ∀ s, W.grp s = g → W.place (σ.openEpoch p) s = p → edgeCur σ.edges s o = false)
      (hw : P.reportNeedsWholeGroup = true → Whole W g p) : Step W P σ (σ.report p o g mk)
  | reclaim (σ : State) (src o g k r : Nat) (h : Msg.report src o g k r ∈ σ.msgs) :
      Step W P σ (σ.onReport W P o g k r)
  | tombstone (σ : State) (o : Nat) : Step W P σ (σ.tombstone W o)
  | crash (σ : State) (p : Nat) : Step W P σ σ

/-- Any number of steps. -/
inductive Steps (W : World) (P : Protocol) : State → State → Prop where
  | refl (σ : State) : Steps W P σ σ
  | tail {σ₁ σ₂ σ₃ : State} : Steps W P σ₁ σ₂ → Step W P σ₂ σ₃ → Steps W P σ₁ σ₃

/-- A state some run reaches from the empty cluster, through any interleaving of steps. -/
def Reachable (W : World) (P : Protocol) (σ : State) : Prop := Steps W P State.init σ

theorem Steps.trans {W : World} {P : Protocol} {σ₁ σ₂ σ₃ : State} (h₁ : Steps W P σ₁ σ₂) (h₂ : Steps W P σ₂ σ₃) :
    Steps W P σ₁ σ₃ := by
  induction h₂ with
  | refl => exact h₁
  | tail _ hs ih => exact Steps.tail ih hs

theorem Reachable.steps {W : World} {P : Protocol} {σ₁ σ₂ : State} (h : Reachable W P σ₁)
    (hs : Steps W P σ₁ σ₂) : Reachable W P σ₂ := Steps.trans h hs

end PlacementModel
