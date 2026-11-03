/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import PlacementModel.Repaired

/-!
# P24's rules 4, 5, 6, 9 and 11, and placement activation

Each theorem holds of every reachable state, so for every number of partitions, entities, bucket groups,
registrations and steps, under every interleaving, delay, duplication or loss of messages and every crash. Rules 4 and
11 hold for every protocol variant, P24 as written included. Rules 5, 6 and 9 hold for `Protocol.repaired`;
`PlacementModel.Counterexamples` shows each repair is needed.

Each carries, under **Simulator invariant**, the check group 7's simulator makes after every step against its partition
logs, referrer sets, marks and delivered closes.

| Rule | Theorem |
|---|---|
| 4, the epoch inequality | `epoch_inequality` |
| 5 and 6, reverse reads | `reverse_read_complete` |
| 9, reclamation | `reclamation_keeps_every_member_with_an_edge`, `a_stale_reclamation_report_removes_nothing` |
| 11, catalog activation | `no_stamp_before_its_catalog_version` |
| 11 for placement, placement activation | `no_route_before_its_placement_version`, `placement_epochs_rise` |
| placement fixed once announced | `placement_in_effect_is_fixed`, `routes_through_a_held_version` |
| the read's ordering | `stamps_never_fall_along_a_key` |

Placement activation and its fixity hold for every protocol variant. `Placed` — the world's placement map agreeing with
the versioned placement at every announced epoch — is a premise of the theorems connecting the two
(`routes_through_a_held_version`, `handover_reads_fixed_versions`, `readReady_reads_fixed_versions`); `Handover` stays a
premise of the write steps.
-/

namespace PlacementModel

/-! ## What a run appends -/

/-- From `σ` to `σ'`, no delivered close falls, and every edge and member event appended is stamped above the
delivered close its writer had in `σ`. -/
def Extends (W : World) (σ σ' : State) : Prop :=
  (∀ p, σ.delivered p ≤ σ'.delivered p) ∧
    (∃ X, σ'.edges = σ.edges ++ X ∧ ∀ ev ∈ X, σ.delivered ev.part < ev.stamp ∧ ev.part = W.place ev.stamp ev.subj) ∧
    (∃ Y, σ'.mems = σ.mems ++ Y ∧ ∀ ev ∈ Y, σ.delivered (W.home ev.obj) < ev.epoch)

theorem Extends.refl (W : World) (σ : State) : Extends W σ σ :=
  ⟨fun _ => Nat.le_refl _, ⟨[], by simp, by simp⟩, ⟨[], by simp, by simp⟩⟩

theorem Extends.trans {W : World} {σ₁ σ₂ σ₃ : State} (h₁ : Extends W σ₁ σ₂) (h₂ : Extends W σ₂ σ₃) :
    Extends W σ₁ σ₃ := by
  obtain ⟨d₁, ⟨X₁, e₁, x₁⟩, ⟨Y₁, m₁, y₁⟩⟩ := h₁
  obtain ⟨d₂, ⟨X₂, e₂, x₂⟩, ⟨Y₂, m₂, y₂⟩⟩ := h₂
  refine ⟨fun p => Nat.le_trans (d₁ p) (d₂ p), ⟨X₁ ++ X₂, by rw [e₂, e₁, List.append_assoc], ?_⟩,
    ⟨Y₁ ++ Y₂, by rw [m₂, m₁, List.append_assoc], ?_⟩⟩
  · intro ev hev
    rcases List.mem_append.mp hev with h | h
    · exact x₁ ev h
    · exact ⟨Nat.lt_of_le_of_lt (d₁ _) (x₂ ev h).1, (x₂ ev h).2⟩
  · intro ev hev
    rcases List.mem_append.mp hev with h | h
    · exact y₁ ev h
    · exact Nat.lt_of_le_of_lt (d₁ _) (y₂ ev h)

/-- A step that appends one edge event of `p`'s subject at `p`'s open epoch. -/
theorem Extends.edge {W : World} {σ : State} {p s o : Nat} (b : Bool) (hp : W.place (σ.openEpoch p) s = p)
    (σ' : State) (hd : σ'.delivered = σ.delivered) (he : σ'.edges = σ.edges ++ [⟨s, o, σ.openEpoch p, p, b⟩])
    (hm : σ'.mems = σ.mems) : Extends W σ σ' :=
  ⟨fun q => by rw [hd]; exact Nat.le_refl _,
    ⟨_, he, fun ev hev => by
      rw [List.mem_singleton] at hev; subst hev
      exact ⟨Nat.lt_succ_self _, hp.symm⟩⟩,
    ⟨[], by rw [hm, List.append_nil], by simp⟩⟩

/-- A step that appends one member event of `o` at its home's open epoch, and no edge event. -/
theorem Extends.member {W : World} {σ : State} {o : Nat} (x : MemEv) (ho : x.obj = o)
    (hx : x.epoch = σ.openEpoch (W.home o)) (σ' : State) (hd : σ'.delivered = σ.delivered)
    (he : σ'.edges = σ.edges) (hm : σ'.mems = σ.mems ++ [x]) : Extends W σ σ' :=
  ⟨fun q => by rw [hd]; exact Nat.le_refl _, ⟨[], by rw [he, List.append_nil], by simp⟩,
    ⟨_, hm, fun ev hev => by
      rw [List.mem_singleton] at hev; subst hev
      rw [hx, ho]; exact Nat.lt_succ_self _⟩⟩

/-- A step that appends no event at all. -/
theorem Extends.none {W : World} {σ σ' : State} (hd : ∀ p, σ.delivered p ≤ σ'.delivered p)
    (he : σ'.edges = σ.edges) (hm : σ'.mems = σ.mems) : Extends W σ σ' :=
  ⟨hd, ⟨[], by rw [he, List.append_nil], by simp⟩, ⟨[], by rw [hm, List.append_nil], by simp⟩⟩

theorem Step.extends {W : World} {P : Protocol} {σ σ' : State} (hs : Step W P σ σ') : Extends W σ σ' := by
  cases hs with
  | propose v => exact Extends.none (fun _ => Nat.le_refl _) rfl rfl
  | proposePlacement v => exact Extends.none (fun _ => Nat.le_refl _) rfl rfl
  | close => exact Extends.none (fun _ => Nat.le_refl _) rfl rfl
  | deliver p _ =>
    refine Extends.none (fun q => ?_) rfl rfl
    show σ.delivered q ≤ upd1 σ.delivered p (σ.delivered p + 1) q
    by_cases hq : q = p
    · subst hq; rw [upd1_self]; omega
    · rw [upd1_other _ _ hq]; exact Nat.le_refl _
  | register => exact Extends.none (fun _ => Nat.le_refl _) rfl rfl
  | accept src o g n mv _ =>
    unfold State.accept
    split
    · exact Extends.none (fun _ => Nat.le_refl _) rfl rfl
    · split
      · exact Extends.none (fun _ => Nat.le_refl _) rfl rfl
      · exact Extends.member _ rfl rfl _ rfl rfl rfl
  | insertHome p s o hp hh _ =>
    obtain ⟨d, ⟨X, e, x⟩, -⟩ := Extends.edge (W := W) (o := o) true hp
      ({ σ with edges := σ.edges ++ [⟨s, o, σ.openEpoch p, p, true⟩] }) rfl rfl rfl
    unfold State.insertHome
    split
    · exact ⟨d, ⟨X, e, x⟩, ⟨[], by simp, by simp⟩⟩
    · refine ⟨d, ⟨X, e, x⟩, ⟨_, rfl, fun ev hev => ?_⟩⟩
      rw [List.mem_singleton] at hev; subst hev
      show σ.delivered (W.home o) < σ.openEpoch p
      rw [hh]; exact Nat.lt_succ_self _
  | insertMark p s o mk hp _ _ _ => exact Extends.edge true hp _ rfl rfl rfl
  | insertReply p s o gen c mv hp _ _ _ _ _ => exact Extends.edge true hp _ rfl rfl rfl
  | delete p s o hp _ => exact Extends.edge false hp _ rfl rfl rfl
  | report => exact Extends.none (fun _ => Nat.le_refl _) rfl rfl
  | reclaim src o g k r _ =>
    unfold State.onReport
    split
    · exact Extends.member _ rfl rfl _ rfl rfl rfl
    · exact Extends.refl W σ
  | tombstone => exact Extends.none (fun _ => Nat.le_refl _) rfl rfl
  | crash => exact Extends.refl W σ

theorem Steps.extends {W : World} {P : Protocol} {σ σ' : State} (hs : Steps W P σ σ') : Extends W σ σ' := by
  induction hs with
  | refl => exact Extends.refl W _
  | tail _ hstep ih => exact ih.trans hstep.extends

/-! ## Rule 4: the epoch inequality -/

/-- **The epoch inequality** (P24 rule 4). Every commit, on every write path — a registrant's commit under a fresh reply
or under its mark, the home's own commit adding its group, and the home's admission of a registration — is stamped at
or above the epoch of every registration it references, and that registration is a member event of the entity's
referrer set at the home, added or renewed at or below that epoch. It holds for P24 as written.

**Simulator invariant:** for every commit in every partition log, and every reference to a registration `(o, g, a)` it
carries, `a` is at or below the commit's epoch, and `R(o)` at `o`'s home has an add or renew event for `g` at an epoch
at or below `a`. -/
theorem epoch_inequality {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ) {rec : CommitRec}
    (hrec : rec ∈ σ.commits) {r : Ref} (hr : r ∈ rec.refs) :
    r.epoch ≤ rec.stamp ∧ HasReg σ.mems r.obj r.grp r.epoch :=
  (h.base.commitsOk rec hrec).2.2 r hr

/-- A partition stamps every commit with exactly one above its latest delivered close: the stamps it can still write
are above every close it holds. -/
theorem stamps_above_delivered_close {W : World} {P : Protocol} {σ σ' : State} (hs : Steps W P σ σ') :
    (∀ ev ∈ σ'.edges, ev ∉ σ.edges → σ.delivered ev.part < ev.stamp) ∧
      (∀ ev ∈ σ'.mems, ev ∉ σ.mems → σ.delivered (W.home ev.obj) < ev.epoch) := by
  obtain ⟨-, ⟨X, e, x⟩, ⟨Y, m, y⟩⟩ := hs.extends
  refine ⟨fun ev hev hn => ?_, fun ev hev hn => ?_⟩
  · rw [e] at hev
    rcases List.mem_append.mp hev with h | h
    · exact absurd h hn
    · exact (x ev h).1
  · rw [m] at hev
    rcases List.mem_append.mp hev with h | h
    · exact absurd h hn
    · exact y ev h

/-! ## Rules 5 and 6: reverse reads at a closed epoch -/

/-- A reverse read of `o` at epoch `e` can answer: `o`'s home has closed `e`, and for every subject whose group
`R(o)` names at `e`, every partition that held its bucket at an epoch `t ≤ e` has closed `t` — the partition the map
gives it at `e` has closed `e`, and a move's earlier holders have handed it over. -/
def ReadReady (W : World) (σ : State) (o e : Nat) : Prop :=
  e ≤ σ.delivered (W.home o) ∧ ∀ s, memAt σ.mems o (W.grp s) e = true → ∀ t, t ≤ e → t ≤ σ.delivered (W.place t s)

/-- The answer of a reverse read of `o` at `e`: the subjects whose group `R(o)` names at `e` and whose partition's
local reverse index holds an edge from them to `o` at `e`. -/
def reverseRead (W : World) (σ : State) (o e s : Nat) : Bool := memAt σ.mems o (W.grp s) e && edgeAt σ.edges s o e

/-- **A reverse read at a closed epoch is complete** (P24 rules 5 and 6, with rule 4). A reverse read of `o` at epoch
`e`, answered in a reachable state `σ` once `o`'s home and the partitions `R(o)` names at `e` have closed `e`, returns
every edge to `o` that any later state shows at `e`: every edge committed at or below `e`, before the read or after it.

**Simulator invariant:** for every reverse read of `o` at a closed epoch `e` the simulator answers at step `i`, and at
every later step `j`, every subject whose edge to `o` the single-partition reference model holds at `e` after step `j`
is in the read's answer: its group is in `R(o)` at `e` at the home, and the partition the map gives its bucket at `e`
holds the edge in its local reverse index at `e`. -/
theorem reverse_read_complete {W : World} {σ σ' : State} (h : Reachable W Protocol.repaired σ)
    (hs : Steps W Protocol.repaired σ σ') {o e : Nat} (hready : ReadReady W σ o e) {s : Nat}
    (he : edgeAt σ'.edges s o e = true) : reverseRead W σ o e s = true := by
  obtain ⟨-, ⟨X, hX, hx⟩, ⟨Y, hY, hy⟩⟩ := hs.extends
  have hm' := (h.steps hs).rep.complete s o e he
  -- Member events appended after `σ` are stamped above the home's close, which is at or above `e`.
  have hm : memAt σ.mems o (W.grp s) e = true := by
    rw [hY] at hm'
    unfold memAt at hm' ⊢
    rw [visible_append_none _ _ _ _ (fun ev hev => by
      have := hy ev hev
      simp only [memKey, decide_eq_false_iff_not]
      rintro ⟨ho, _, hle⟩
      rw [ho] at this
      have := hready.1
      omega)] at hm'
    exact hm'
  -- Edge events of `s` appended after `σ` are stamped above every epoch its holders had closed, so above `e`.
  have hedge : edgeAt σ.edges s o e = true := by
    rw [hX] at he
    unfold edgeAt at he ⊢
    rw [visible_append_none _ _ _ _ (fun ev hev => by
      obtain ⟨hlt, hpart⟩ := hx ev hev
      simp only [edgeKey, decide_eq_false_iff_not]
      rintro ⟨hs', _, hle⟩
      have := hready.2 s hm ev.stamp hle
      rw [← hs', ← hpart] at this
      omega)] at he
    exact he
  simp [reverseRead, hm, hedge]

/-! ## Rule 9: reclamation -/

/-- **Reclamation never removes a member whose group holds an edge to the entity at an epoch the history retains**
(P24 rule 9). In every reachable state, whenever the edge from `s` to `o` is visible at an epoch `e`, `s`'s group is a
member of `R(o)` at `e`. The model retains every epoch, so this covers every epoch a read may name; physical deletion
below the history-retention horizon removes only what no read can name.

**Simulator invariant:** after every step, for every partition, every edge `s → o` its local reverse index holds at an
epoch `e`, and every `e` from the oldest retained epoch to the open one, `R(o)` at `o`'s home holds `s`'s bucket group
at `e`. -/
theorem reclamation_keeps_every_member_with_an_edge {W : World} {σ : State} (h : Reachable W Protocol.repaired σ)
    {s o e : Nat} (he : edgeAt σ.edges s o e = true) : memAt σ.mems o (W.grp s) e = true :=
  h.rep.complete s o e he

/-- **A stale reclamation report removes nothing.** When a report names generation `k` and the member stands at
another generation — the partition re-registered before the report arrived, and the home raised the member — applying
the report changes nothing, under every protocol variant; and in the repaired protocol the reverse-read completeness
theorem keeps covering every edge, the partition's new edge included.

**Simulator invariant:** whenever the home applies a report whose generation differs from the member's current one,
the home's referrer set and log are unchanged by it, and the check of `reclamation_keeps_every_member_with_an_edge`
passes after the step. -/
theorem a_stale_reclamation_report_removes_nothing {W : World} {σ : State} {o g k r : Nat} {ev : MemEv}
    (hcur : memCur σ.mems o g = some ev) (hgen : ev.gen ≠ k) :
    (∀ P : Protocol, σ.onReport W P o g k r = σ) ∧
      (Reachable W Protocol.repaired σ → ∀ s e, edgeAt σ.edges s o e = true → memAt σ.mems o (W.grp s) e = true) := by
  refine ⟨fun P => ?_, fun h s e he => reclamation_keeps_every_member_with_an_edge h he⟩
  have : σ.curIs o g k = false := by
    unfold State.curIs; rw [hcur]; simp [hgen]
  unfold State.onReport; rw [this]; rfl

/-! ## Rule 11: catalog activation -/

/-- **No partition stamps a catalog version's effective epoch before it holds the version** (P24 rule 11). A version
committed as pending is assigned `E = c + 2` by the close `c + 1` that announces it; every commit stamped at or above
`E`, at any partition and on any write path, was made while its partition held the version. It holds for P24 as
written.

**Simulator invariant:** for every commit in every partition log stamped `t`, and every catalog version whose
effective epoch in the sequencer's close records is at or below `t`, the partition had loaded that version when it
committed. -/
theorem no_stamp_before_its_catalog_version {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ)
    {rec : CommitRec} (hrec : rec ∈ σ.commits) {x : Nat × Nat} (hx : x ∈ σ.assigned) (hle : x.2 ≤ rec.stamp) :
    x.1 ∈ rec.held :=
  (h.base.commitsOk rec hrec).2.1 x hx hle

/-- Every partition holds every version taking effect at or below its open epoch, so it holds a version before it can
stamp its effective epoch. -/
theorem partition_holds_versions_of_its_open_epoch {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ)
    (p : Nat) {x : Nat × Nat} (hx : x ∈ σ.assigned) (hle : x.2 ≤ σ.openEpoch p) : x.1 ∈ σ.held p :=
  h.base.heldAll p x hx hle

/-- A close announces exactly the versions pending when it is issued, at one above its own number. -/
theorem close_assigns_one_above (σ : State) {x : Nat × Nat} (hx : x ∈ σ.close.assigned) (hn : x ∉ σ.assigned) :
    x.1 ∈ σ.pending ∧ x.2 = (σ.seq + 1) + 1 := by
  rcases List.mem_append.mp hx with h | h
  · exact absurd h hn
  · simp only [List.mem_map] at h
    obtain ⟨v, hv, rfl⟩ := h
    exact ⟨hv, rfl⟩

/-! ## Placement activation: the rule-11 analogue for placement versions -/

/-- **No partition routes at a placement version's effective epoch before it holds the version.** A placement version
committed as pending is assigned `E = c + 2` by the close `c + 1` that announces it; every commit stamped at or above
`E`, at any partition and on any write path, was made while its partition held the version. It holds for P24 as
written.

**Simulator invariant:** for every commit in every partition log stamped `t`, and every placement version whose
effective epoch in the sequencer's close records is at or below `t`, the partition had loaded that version when it
committed. -/
theorem no_route_before_its_placement_version {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ)
    {rec : CommitRec} (hrec : rec ∈ σ.commits) {x : Nat × Nat} (hx : x ∈ σ.passigned) (hle : x.2 ≤ rec.stamp) :
    x.1 ∈ rec.pheld :=
  h.place.pcommitsOk rec hrec x hx hle

/-- Every partition holds every placement version taking effect at or below its open epoch. -/
theorem partition_holds_placement_versions_of_its_open_epoch {W : World} {P : Protocol} {σ : State}
    (h : Reachable W P σ) (p : Nat) {x : Nat × Nat} (hx : x ∈ σ.passigned) (hle : x.2 ≤ σ.openEpoch p) :
    x.1 ∈ σ.pheld p :=
  h.place.pheldAll p x hx hle

/-- A close announces at most one placement version, the oldest pending one, at one above its own number, and drops it
from the pending ones. -/
theorem close_assigns_one_placement_above (σ : State) :
    ∃ X, σ.close.passigned = σ.passigned ++ X ∧ X.length ≤ 1 ∧
      (∀ x ∈ X, σ.ppending.head? = some x.1 ∧ x.2 = (σ.seq + 1) + 1) ∧ σ.close.ppending = σ.ppending.tail := by
  refine ⟨_, rfl, ?_, ?_, ?_⟩
  · simp only [List.length_map, List.length_take]; omega
  · intro x hx
    change x ∈ (σ.ppending.take 1).map (fun v => (v, σ.seq + 2)) at hx
    cases hp : σ.ppending with
    | nil => rw [hp] at hx; cases hx
    | cons v l =>
      rw [hp] at hx
      simp only [List.take_succ_cons, List.take_zero, List.map_cons, List.map_nil, List.mem_singleton] at hx
      subst hx; exact ⟨rfl, rfl⟩
  · show σ.ppending.drop 1 = σ.ppending.tail
    cases σ.ppending <;> rfl

/-- Two elements of a list whose second components strictly rise along it, with equal second components, are one. -/
theorem pairwise_lt_unique {l : List (Nat × Nat)} (h : l.Pairwise (fun a b => a.2 < b.2)) {x y : Nat × Nat}
    (hx : x ∈ l) (hy : y ∈ l) (he : x.2 = y.2) : x = y := by
  induction l with
  | nil => cases hx
  | cons a l ih =>
    rw [List.pairwise_cons] at h
    rcases List.mem_cons.mp hx with rfl | hx'
    · rcases List.mem_cons.mp hy with rfl | hy'
      · rfl
      · have := h.1 y hy'; omega
    · rcases List.mem_cons.mp hy with rfl | hy'
      · have := h.1 x hx'; omega
      · exact ih h.2 hx' hy'

/-- **Effective epochs of placement versions strictly rise in assignment order**, so no two placement versions share
an effective epoch and each epoch has one version in effect. It holds for P24 as written.

**Simulator invariant:** the sequencer's close records announce placement versions at strictly rising effective
epochs, at most one per close. -/
theorem placement_epochs_rise {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ) :
    σ.passigned.Pairwise (fun a b => a.2 < b.2) ∧
      ∀ x ∈ σ.passigned, ∀ y ∈ σ.passigned, x.2 = y.2 → x = y :=
  ⟨h.place.rise, fun _ hx _ hy he => pairwise_lt_unique h.place.rise hx hy he⟩

/-! ## Placement in effect is fixed once announced -/

/-- From `σ` to `σ'` the sequencer's close does not fall, and every placement version assigned after `σ` takes effect
above `σ.seq + 1`. -/
def PExtends (σ σ' : State) : Prop :=
  σ.seq ≤ σ'.seq ∧ ∃ X, σ'.passigned = σ.passigned ++ X ∧ ∀ x ∈ X, σ.seq + 1 < x.2

theorem PExtends.none {σ σ' : State} (hseq : σ'.seq = σ.seq) (ha : σ'.passigned = σ.passigned) : PExtends σ σ' :=
  ⟨by rw [hseq]; exact Nat.le_refl _, [], by rw [ha, List.append_nil], by simp⟩

theorem Step.pextends {W : World} {P : Protocol} {σ σ' : State} (hs : Step W P σ σ') : PExtends σ σ' := by
  cases hs with
  | close =>
    refine ⟨Nat.le_succ _, _, rfl, fun x hx => ?_⟩
    simp only [List.mem_map] at hx; obtain ⟨v, _, rfl⟩ := hx
    show σ.seq + 1 < σ.seq + 2; omega
  | accept src o g n mv _ =>
    unfold State.accept
    split
    · exact PExtends.none rfl rfl
    · split
      · exact PExtends.none rfl rfl
      · exact PExtends.none rfl rfl
  | reclaim src o g k r _ =>
    unfold State.onReport
    split
    · exact PExtends.none rfl rfl
    · exact PExtends.none rfl rfl
  | _ => exact PExtends.none rfl rfl

theorem Steps.pextends {W : World} {P : Protocol} {σ σ' : State} (hs : Steps W P σ σ') : PExtends σ σ' := by
  induction hs with
  | refl => exact PExtends.none rfl rfl
  | tail _ hstep ih =>
    obtain ⟨s1, X1, e1, x1⟩ := ih
    obtain ⟨s2, X2, e2, x2⟩ := hstep.pextends
    refine ⟨Nat.le_trans s1 s2, X1 ++ X2, by rw [e2, e1, List.append_assoc], fun x hx => ?_⟩
    rcases List.mem_append.mp hx with h | h
    · exact x1 x h
    · have := x2 x h; omega

/-- **The placement version in effect at an announced epoch never changes.** Once the sequencer has issued close
`seq`, no later step of any run changes which placement version is in effect at any epoch at or below `seq + 1`. It
holds for every protocol variant, with no invariant needed.

**Simulator invariant:** for every epoch at or below one above the sequencer's latest close, the placement version the
close records put in effect there is the same after every later step. -/
theorem placement_in_effect_is_fixed {W : World} {P : Protocol} {σ σ' : State} (hs : Steps W P σ σ') {t : Nat}
    (ht : t ≤ σ.seq + 1) : σ'.versionAt t = σ.versionAt t := by
  obtain ⟨_, X, e, x⟩ := hs.pextends
  unfold State.versionAt
  rw [e, lastWhere_append_none _ _ _ (fun y hy => by have := x y hy; simp; omega)]

/-- Every partition's open epoch is announced: placement at it, and below it, is fixed for the rest of the run. -/
theorem placement_fixed_at_open_epochs {W : World} {P : Protocol} {σ σ' : State} (h : Reachable W P σ)
    (hs : Steps W P σ σ') (p : Nat) {t : Nat} (ht : t ≤ σ.openEpoch p) : σ'.versionAt t = σ.versionAt t :=
  placement_in_effect_is_fixed hs (by have := h.base.delivLe p; simp only [State.openEpoch] at ht; omega)

/-- A run whose world agrees with the versioned placement at its end agreed with it at every earlier state. -/
theorem Placed.of_steps {W : World} {P : Protocol} {σ σ' : State} (hs : Steps W P σ σ') (hp : Placed W σ') :
    Placed W σ := by
  intro t ht s
  rw [← placement_in_effect_is_fixed hs ht]
  exact hp t (Nat.le_trans ht (Nat.succ_le_succ hs.pextends.1)) s

/-- The placement version in effect at an epoch at or below a partition's open epoch is one it holds. -/
theorem versionAt_held {σ : State} (h : PlaceInv σ) {p t : Nat} (ht : t ≤ σ.delivered p + 1) :
    σ.versionAt t ∈ σ.pheld p := by
  unfold State.versionAt
  cases hl : lastWhere σ.passigned (fun x => decide (x.2 ≤ t)) with
  | some x =>
    obtain ⟨hx, hq⟩ := lastWhere_mem hl
    simp only [decide_eq_true_eq] at hq
    exact h.pheldAll p x hx (Nat.le_trans hq ht)
  | none =>
    have := lastWhere_none hl _ h.initial
    simp at this

/-- **A partition routes at its open epoch through the placement version in effect there, which it holds.** When the
world's placement map is the versioned one (`Placed`), the partition the map gives a subject at `p`'s open epoch is the
one the version in effect there gives it, and `p` holds that version. -/
theorem routes_through_a_held_version {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ)
    (hp : Placed W σ) (p s : Nat) :
    W.place (σ.openEpoch p) s = W.layout (σ.versionAt (σ.openEpoch p)) s ∧
      σ.versionAt (σ.openEpoch p) ∈ σ.pheld p :=
  ⟨hp _ (by have := h.base.delivLe p; simp only [State.openEpoch]; omega) s, versionAt_held h.place (Nat.le_refl _)⟩

/-- Every commit held the placement version in effect at its stamp. -/
theorem commit_held_its_placement_version {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ)
    {rec : CommitRec} (hrec : rec ∈ σ.commits) : σ.versionAt rec.stamp ∈ rec.pheld := by
  unfold State.versionAt
  cases hl : lastWhere σ.passigned (fun x => decide (x.2 ≤ rec.stamp)) with
  | some x =>
    obtain ⟨hx, hq⟩ := lastWhere_mem hl
    simp only [decide_eq_true_eq] at hq
    exact h.place.pcommitsOk rec hrec x hx hq
  | none =>
    have := lastWhere_none hl _ h.place.initial
    simp at this

/-- **The hand-over reads only announced placement.** Under `Placed`, `Handover` up to an epoch at most two above the
sequencer's close — every partition's open epoch is one — names, at every earlier epoch, the partition of the placement
version in effect there, which `placement_in_effect_is_fixed` keeps for the rest of the run. -/
theorem handover_reads_fixed_versions {W : World} {σ : State} (hp : Placed W σ) (d : Nat → Nat) (s : Nat) {t : Nat}
    (ht : t ≤ σ.seq + 2) :
    Handover W d s t ↔ ∀ t', t' < t → t' ≤ d (W.layout (σ.versionAt t') s) := by
  constructor
  · intro hh t' ht'; rw [← hp t' (by omega) s]; exact hh t' ht'
  · intro hh t' ht'; rw [hp t' (by omega) s]; exact hh t' ht'

/-- **A reverse read's readiness reads only announced placement.** Under `Placed`, `ReadReady` names, at every epoch
at or below the read's, the partition of the placement version in effect there. -/
theorem readReady_reads_fixed_versions {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ)
    (hp : Placed W σ) (o e : Nat) :
    ReadReady W σ o e ↔ e ≤ σ.delivered (W.home o) ∧
      ∀ s, memAt σ.mems o (W.grp s) e = true → ∀ t, t ≤ e → t ≤ σ.delivered (W.layout (σ.versionAt t) s) := by
  constructor
  · intro ⟨he, hr⟩
    have := h.base.delivLe (W.home o)
    exact ⟨he, fun s hm t ht => by rw [← hp t (by omega) s]; exact hr s hm t ht⟩
  · intro ⟨he, hr⟩
    have := h.base.delivLe (W.home o)
    exact ⟨he, fun s hm t ht => by rw [hp t (by omega) s]; exact hr s hm t ht⟩

/-! ## Why the latest event is the right one -/

/-- **Stamps never fall along a key.** Along the edge list, a subject's stamps never decrease, and along the member
list, an entity's epochs never decrease: the event appended last at or below an epoch is the one with the highest
version at or below it, which is what MVCC reads. It holds for P24 as written.

**Simulator invariant:** every partition log's commits for one subject, and every home's referrer-set writes for one
entity, carry non-decreasing epochs in log order, across moves of the subject's bucket. -/
theorem stamps_never_fall_along_a_key {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ) :
    σ.edges.Pairwise (fun a b => a.subj = b.subj → a.stamp ≤ b.stamp) ∧
      σ.mems.Pairwise (fun a b => a.obj = b.obj → a.epoch ≤ b.epoch) :=
  ⟨h.base.edgeSorted, h.base.memSorted⟩

/-! ## Tombstones -/

/-- A registration the home answers after an entity's tombstone adds no member to its referrer set. -/
theorem registration_after_tombstone_adds_nothing {W : World} {P : Protocol} {σ : State} {src o g n mv : Nat}
    (ht : σ.tombstoned o = true) : (σ.accept W P src o g n mv).mems = σ.mems := by
  unfold State.accept; rw [if_pos ht]

end PlacementModel
