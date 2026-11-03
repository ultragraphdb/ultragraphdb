/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import PlacementModel.State

/-!
# The invariant every variant of the protocol keeps

`BaseInv` holds of every reachable state of every `Protocol`, the design as written included: every partition's
delivered close is one the sequencer issued; a partition holds every catalog version whose epoch it can stamp; every
commit's stamp is at or above the epoch of each registration it references, and that registration exists at the home;
every event is stamped at most one above its writer's latest delivered close; every key's stamps never decrease along
its list; and marks, replies, registrations and reports agree with the home's referrer set.

Rules 4 and 11 follow from it in `PlacementModel.Theorems`, as does the stability a reverse read rests on.
-/

namespace PlacementModel

/-- Every commit is stamped at most one above the sequencer's latest close, held every version taking effect at or
below its stamp, and stamps at or above every registration it references, which exists. -/
def CommitsOk (seq : Nat) (assigned : List (Nat × Nat)) (mems : List MemEv) (commits : List CommitRec) : Prop :=
  ∀ rec ∈ commits, rec.stamp ≤ seq + 1 ∧ (∀ x ∈ assigned, x.2 ≤ rec.stamp → x.1 ∈ rec.held) ∧
    (∀ r ∈ rec.refs, r.epoch ≤ rec.stamp ∧ HasReg mems r.obj r.grp r.epoch)

/-- What every reachable state satisfies, whatever the protocol variant. -/
structure BaseInv (W : World) (σ : State) : Prop where
  /-- Every delivered close was issued. -/
  delivLe : ∀ p, σ.delivered p ≤ σ.seq
  /-- Every assigned effective epoch is one above an issued close. -/
  assignedLe : ∀ x ∈ σ.assigned, x.2 ≤ σ.seq + 1
  /-- A partition holds every version taking effect at or below its open epoch. -/
  heldAll : ∀ p, ∀ x ∈ σ.assigned, x.2 ≤ σ.delivered p + 1 → x.1 ∈ σ.held p
  /-- Commits: catalog versions held, and the epoch inequality. -/
  commitsOk : CommitsOk σ.seq σ.assigned σ.mems σ.commits
  /-- A member event is stamped at most at its home's open epoch. -/
  memEpoch : ∀ ev ∈ σ.mems, ev.epoch ≤ σ.delivered (W.home ev.obj) + 1
  /-- An edge event was written by the partition the map gives its subject at its stamp, at most at that partition's
  open epoch, after the hand-over from every earlier holder. -/
  edgeOk : ∀ ev ∈ σ.edges, ev.part = W.place ev.stamp ev.subj ∧ ev.stamp ≤ σ.delivered ev.part + 1 ∧
    Handover W σ.delivered ev.subj ev.stamp
  /-- A subject's edge stamps never decrease along the list. -/
  edgeSorted : σ.edges.Pairwise (fun a b => a.subj = b.subj → a.stamp ≤ b.stamp)
  /-- An entity's member epochs never decrease along the list. -/
  memSorted : σ.mems.Pairwise (fun a b => a.obj = b.obj → a.epoch ≤ b.epoch)
  /-- A mark rests on a registration at or below its holder's open epoch, names a generation the member has had, and
  is held by a partition that can hold the group and is not the home. -/
  markOk : ∀ p o g mk, σ.marks p o g = some mk → mk.regEpoch ≤ σ.delivered p + 1 ∧ HasReg σ.mems o g mk.regEpoch ∧
    mk.gen ≤ genMax σ.mems o g ∧ CanHold W g p ∧ p ≠ W.home o
  /-- A live reply rests on a registration at or below `c + 1` and names a generation the member has had. -/
  replyOk : ∀ dst o g gen c mv, Msg.reply dst o g gen c mv false ∈ σ.msgs →
    HasReg σ.mems o g (c + 1) ∧ gen ≤ genMax σ.mems o g ∧ mv ≤ σ.mver dst o g ∧ c ≤ σ.delivered (W.home o)
  /-- A registration read the mark key at a version at or below its current one, and named the generation its sender
  had seen if the key is unchanged since. -/
  regOk : ∀ src o g n mv, Msg.reg src o g n mv ∈ σ.msgs →
    mv ≤ σ.mver src o g ∧ (mv = σ.mver src o g → n = σ.seen src o g)
  /-- A report names a generation the member has had, from a partition that can hold the group and is not the home. -/
  reportOk : ∀ src o g k r, Msg.report src o g k r ∈ σ.msgs →
    k ≤ genMax σ.mems o g ∧ CanHold W g src ∧ src ≠ W.home o

/-! ## Small lemmas -/

theorem HasReg.append {l : List MemEv} {o g a : Nat} (h : HasReg l o g a) (X : List MemEv) : HasReg (l ++ X) o g a :=
  let ⟨ev, hm, rest⟩ := h; ⟨ev, List.mem_append_left _ hm, rest⟩

theorem HasReg.mono {l : List MemEv} {o g a b : Nat} (h : HasReg l o g a) (hab : a ≤ b) : HasReg l o g b :=
  let ⟨ev, hm, ho, hg, hp, he⟩ := h; ⟨ev, hm, ho, hg, hp, Nat.le_trans he hab⟩

theorem Handover.mono {W : World} {d d' : Nat → Nat} {s t : Nat} (h : Handover W d s t) (hd : ∀ p, d p ≤ d' p) :
    Handover W d' s t := fun t' ht => Nat.le_trans (h t' ht) (hd _)

theorem pairwise_append_single {β : Type} {R : β → β → Prop} {l : List β} {x : β} (h : l.Pairwise R)
    (hx : ∀ a ∈ l, R a x) : (l ++ [x]).Pairwise R :=
  List.pairwise_append.mpr ⟨h, List.pairwise_singleton _ _, fun a ha b hb => by
    rw [List.mem_singleton] at hb; subst hb; exact hx a ha⟩

theorem genMax_append_mono' (l X : List MemEv) (o g : Nat) : genMax l o g ≤ genMax (l ++ X) o g := by
  induction X generalizing l with
  | nil => simp
  | cons x xs ih =>
    rw [show l ++ x :: xs = (l ++ [x]) ++ xs by simp]
    exact Nat.le_trans (genMax_append_mono l x o g) (ih _)

theorem upd1_self {β : Type} (f : Nat → β) (p : Nat) (v : β) : upd1 f p v p = v := by simp [upd1]

theorem upd1_other {β : Type} (f : Nat → β) {p q : Nat} (v : β) (h : q ≠ p) : upd1 f p v q = f q := by
  simp [upd1, h]

theorem upd3_self {β : Type} (f : Nat → Nat → Nat → β) (p o g : Nat) (v : β) : upd3 f p o g v p o g = v := by
  simp [upd3]

theorem upd3_other {β : Type} (f : Nat → Nat → Nat → β) {p o g p' o' g' : Nat} (v : β)
    (h : ¬(p' = p ∧ o' = o ∧ g' = g)) : upd3 f p o g v p' o' g' = f p' o' g' := by
  simp only [upd3, if_neg h]

/-- Bumping the mark version of one key never lowers a version. -/
theorem mver_bump_ge (f : Nat → Nat → Nat → Nat) (p o g p' o' g' : Nat) :
    f p' o' g' ≤ upd3 f p o g (f p o g + 1) p' o' g' := by
  by_cases h : p' = p ∧ o' = o ∧ g' = g
  · obtain ⟨rfl, rfl, rfl⟩ := h; rw [upd3_self]; omega
  · rw [upd3_other _ _ h]; exact Nat.le_refl _

theorem isRepeat_spec {σ : State} {o g n : Nat} (h : σ.isRepeat o g n = true) :
    ∃ ev, memCur σ.mems o g = some ev ∧ ev.present = true ∧ n < ev.gen ∧ σ.curGen o g = ev.gen := by
  unfold State.isRepeat at h
  unfold State.curGen
  cases hc : memCur σ.mems o g with
  | none => rw [hc] at h; cases h
  | some ev => rw [hc] at h; simp at h; exact ⟨ev, rfl, h.1, h.2, rfl⟩

theorem isMember_spec {σ : State} {o g : Nat} (h : σ.isMember o g = true) :
    ∃ ev, memCur σ.mems o g = some ev ∧ ev.present = true := by
  unfold State.isMember at h
  cases hc : memCur σ.mems o g with
  | none => rw [hc] at h; cases h
  | some ev => rw [hc] at h; exact ⟨ev, rfl, h⟩

theorem curIs_spec {σ : State} {o g k : Nat} (h : σ.curIs o g k = true) :
    ∃ ev, memCur σ.mems o g = some ev ∧ ev.present = true ∧ ev.gen = k := by
  unfold State.curIs at h
  cases hc : memCur σ.mems o g with
  | none => rw [hc] at h; cases h
  | some ev => rw [hc] at h; simp at h; exact ⟨ev, rfl, h.1, h.2⟩

/-- A commit recorded at `p`'s open epoch keeps `CommitsOk`, given its references are at or below its stamp and
exist in the new referrer sets. -/
theorem CommitsOk.record {W : World} {σ : State} (h : BaseInv W σ) {mems' : List MemEv}
    (hext : ∀ o g a, HasReg σ.mems o g a → HasReg mems' o g a) (p : Nat) (refs : List Ref)
    (hrefs : ∀ r ∈ refs, r.epoch ≤ σ.openEpoch p ∧ HasReg mems' r.obj r.grp r.epoch) :
    CommitsOk σ.seq σ.assigned mems' (σ.record p refs) := by
  intro rec hrec
  rcases List.mem_append.mp hrec with hold | hnew
  · obtain ⟨h1, h2, h3⟩ := h.commitsOk rec hold
    exact ⟨h1, h2, fun r hr => ⟨(h3 r hr).1, hext _ _ _ (h3 r hr).2⟩⟩
  · rw [List.mem_singleton] at hnew; subst hnew
    exact ⟨by simp only [State.openEpoch]; have := h.delivLe p; omega,
      fun x hx hle => h.heldAll p x hx hle, hrefs⟩

/-- Extending the member list keeps every existing registration. -/
theorem CommitsOk.extend {seq : Nat} {assigned : List (Nat × Nat)} {mems : List MemEv} {commits : List CommitRec}
    (h : CommitsOk seq assigned mems commits) (X : List MemEv) : CommitsOk seq assigned (mems ++ X) commits :=
  fun rec hrec => let ⟨h1, h2, h3⟩ := h rec hrec; ⟨h1, h2, fun r hr => ⟨(h3 r hr).1, (h3 r hr).2.append X⟩⟩

/-- A new edge event from `p`'s subject `s`, stamped at `p`'s open epoch, keeps the edge invariants. -/
theorem BaseInv.newEdge {W : World} {σ : State} (h : BaseInv W σ) {p s o : Nat} (b : Bool)
    (hp : W.place (σ.openEpoch p) s = p) (hand : Handover W σ.delivered s (σ.openEpoch p)) :
    (∀ ev ∈ σ.edges ++ [(⟨s, o, σ.openEpoch p, p, b⟩ : EdgeEv)], ev.part = W.place ev.stamp ev.subj ∧
      ev.stamp ≤ σ.delivered ev.part + 1 ∧ Handover W σ.delivered ev.subj ev.stamp) ∧
    (σ.edges ++ [(⟨s, o, σ.openEpoch p, p, b⟩ : EdgeEv)]).Pairwise
      (fun x y => x.subj = y.subj → x.stamp ≤ y.stamp) := by
  refine ⟨fun ev hev => ?_, pairwise_append_single h.edgeSorted ?_⟩
  · rcases List.mem_append.mp hev with hold | hnew
    · exact h.edgeOk ev hold
    · rw [List.mem_singleton] at hnew; subst hnew
      exact ⟨hp.symm, by simp [State.openEpoch], hand⟩
  · intro a ha hsub
    simp only at hsub
    obtain ⟨_, _, hh⟩ := h.edgeOk a ha
    show a.stamp ≤ σ.openEpoch p
    apply Nat.le_of_not_lt
    intro hlt
    have := hh (σ.openEpoch p) hlt
    rw [hsub, hp] at this
    simp only [State.openEpoch] at this; omega

/-! ## Preservation, step by step -/

theorem BaseInv.init (W : World) : BaseInv W State.init := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;>
    first
    | exact List.Pairwise.nil
    | (intro p; exact Nat.le_refl _)
    | (simp [State.init, CommitsOk])

theorem BaseInv.propose {W : World} {σ : State} (h : BaseInv W σ) (v : Nat) : BaseInv W (σ.propose v) :=
  { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll, commitsOk := h.commitsOk,
    memEpoch := h.memEpoch, edgeOk := h.edgeOk, edgeSorted := h.edgeSorted, memSorted := h.memSorted,
    markOk := h.markOk, replyOk := h.replyOk, regOk := h.regOk, reportOk := h.reportOk }

theorem BaseInv.close {W : World} {σ : State} (h : BaseInv W σ) : BaseInv W σ.close := by
  have hnew : ∀ x ∈ σ.pending.map (fun v => (v, σ.seq + 2)), x.2 = σ.seq + 2 := by
    intro x hx; simp only [List.mem_map] at hx; obtain ⟨v, _, rfl⟩ := hx; rfl
  refine { delivLe := fun p => Nat.le_succ_of_le (h.delivLe p), assignedLe := ?_, heldAll := ?_, commitsOk := ?_,
           memEpoch := h.memEpoch, edgeOk := h.edgeOk, edgeSorted := h.edgeSorted, memSorted := h.memSorted,
           markOk := h.markOk, replyOk := h.replyOk, regOk := h.regOk, reportOk := h.reportOk }
  · intro x hx
    rcases List.mem_append.mp hx with hold | hn
    · show x.2 ≤ σ.seq + 1 + 1; have := h.assignedLe x hold; omega
    · show x.2 ≤ σ.seq + 1 + 1; rw [hnew x hn]; omega
  · intro p x hx hle
    rcases List.mem_append.mp hx with hold | hn
    · exact h.heldAll p x hold hle
    · have := h.delivLe p; rw [hnew x hn] at hle; exact absurd hle (by show ¬ _ ≤ σ.delivered p + 1; omega)
  · intro rec hrec
    obtain ⟨h1, h2, h3⟩ := h.commitsOk rec hrec
    refine ⟨Nat.le_succ_of_le h1, fun x hx hle => ?_, h3⟩
    rcases List.mem_append.mp hx with hold | hn
    · exact h2 x hold hle
    · rw [hnew x hn] at hle; omega

theorem BaseInv.deliver {W : World} {σ : State} (h : BaseInv W σ) {p : Nat} (hlt : σ.delivered p < σ.seq) :
    BaseInv W (σ.deliver p) := by
  have hmono : ∀ q, σ.delivered q ≤ (σ.deliver p).delivered q := by
    intro q; show σ.delivered q ≤ upd1 σ.delivered p (σ.delivered p + 1) q
    by_cases hq : q = p
    · subst hq; rw [upd1_self]; omega
    · rw [upd1_other _ _ hq]; exact Nat.le_refl _
  refine { delivLe := ?_, assignedLe := h.assignedLe, heldAll := ?_, commitsOk := h.commitsOk,
           memEpoch := fun ev hev => Nat.le_trans (h.memEpoch ev hev) (Nat.succ_le_succ (hmono _)),
           edgeOk := fun ev hev => ?_, edgeSorted := h.edgeSorted, memSorted := h.memSorted,
           markOk := fun q o g mk hm => ?_, replyOk := fun dst o g gen c mv hr => ?_, regOk := h.regOk,
           reportOk := h.reportOk }
  · intro q; show upd1 σ.delivered p (σ.delivered p + 1) q ≤ σ.seq
    by_cases hq : q = p
    · subst hq; rw [upd1_self]; omega
    · rw [upd1_other _ _ hq]; exact h.delivLe q
  · intro q x hx hle
    show x.1 ∈ upd1 σ.held p _ q
    by_cases hq : q = p
    · subst hq
      rw [upd1_self]
      change x.2 ≤ upd1 σ.delivered q (σ.delivered q + 1) q + 1 at hle
      rw [upd1_self] at hle
      by_cases hx2 : x.2 ≤ σ.delivered q + 1
      · exact List.mem_append_left _ (h.heldAll q x hx hx2)
      · apply List.mem_append_right
        simp only [List.mem_map, List.mem_filter]
        exact ⟨x, ⟨hx, by simp; omega⟩, rfl⟩
    · rw [upd1_other _ _ hq]
      change x.2 ≤ upd1 σ.delivered p (σ.delivered p + 1) q + 1 at hle
      rw [upd1_other _ _ hq] at hle
      exact h.heldAll q x hx hle
  · obtain ⟨h1, h2, h3⟩ := h.edgeOk ev hev
    exact ⟨h1, Nat.le_trans h2 (Nat.succ_le_succ (hmono _)), h3.mono hmono⟩
  · obtain ⟨h1, h2⟩ := h.markOk q o g mk hm
    exact ⟨Nat.le_trans h1 (Nat.succ_le_succ (hmono _)), h2⟩
  · obtain ⟨h1, h2, h3, h4⟩ := h.replyOk dst o g gen c mv hr
    exact ⟨h1, h2, h3, Nat.le_trans h4 (hmono _)⟩

theorem BaseInv.register {W : World} {σ : State} (h : BaseInv W σ) (p o g : Nat) : BaseInv W (σ.register p o g) := by
  refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll, commitsOk := h.commitsOk,
           memEpoch := h.memEpoch, edgeOk := h.edgeOk, edgeSorted := h.edgeSorted, memSorted := h.memSorted,
           markOk := h.markOk, replyOk := ?_, regOk := ?_, reportOk := ?_ }
  · intro dst o' g' gen c mv hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.replyOk dst o' g' gen c mv hold
    · simp at hn
  · intro src o' g' n mv hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.regOk src o' g' n mv hold
    · simp only [List.mem_singleton, Msg.reg.injEq] at hn
      obtain ⟨rfl, rfl, rfl, rfl, rfl⟩ := hn
      exact ⟨Nat.le_refl _, fun _ => rfl⟩
  · intro src o' g' k r hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.reportOk src o' g' k r hold
    · simp at hn

theorem BaseInv.acceptTomb {W : World} {σ : State} (h : BaseInv W σ) (src o g mv : Nat) :
    BaseInv W { σ with msgs := σ.msgs ++ [.reply src o g 0 (σ.delivered (W.home o)) mv true] } := by
  refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll, commitsOk := h.commitsOk,
           memEpoch := h.memEpoch, edgeOk := h.edgeOk, edgeSorted := h.edgeSorted, memSorted := h.memSorted,
           markOk := h.markOk, replyOk := ?_, regOk := ?_, reportOk := ?_ }
  · intro dst o' g' gen c mv' hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.replyOk dst o' g' gen c mv' hold
    · simp at hn
  · intro src' o' g' n mv' hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.regOk src' o' g' n mv' hold
    · simp at hn
  · intro src' o' g' k r hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.reportOk src' o' g' k r hold
    · simp at hn

theorem BaseInv.acceptRepeat {W : World} {σ : State} (h : BaseInv W σ) {src o g n mv : Nat}
    (hreg : Msg.reg src o g n mv ∈ σ.msgs) (hrep : σ.isRepeat o g n = true) :
    BaseInv W { σ with msgs := σ.msgs ++ [.reply src o g (σ.curGen o g) (σ.delivered (W.home o)) mv false] } := by
  obtain ⟨ev, hc, hp, _, hg⟩ := isRepeat_spec hrep
  obtain ⟨hm, ho, hgg⟩ := memCur_mem hc
  refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll, commitsOk := h.commitsOk,
           memEpoch := h.memEpoch, edgeOk := h.edgeOk, edgeSorted := h.edgeSorted, memSorted := h.memSorted,
           markOk := h.markOk, replyOk := ?_, regOk := ?_, reportOk := ?_ }
  · intro dst o' g' gen c mv' hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.replyOk dst o' g' gen c mv' hold
    · simp only [List.mem_singleton, Msg.reply.injEq] at hn
      obtain ⟨rfl, rfl, rfl, rfl, rfl, rfl, -⟩ := hn
      refine ⟨⟨ev, hm, ho, hgg, hp, ?_⟩, ?_, (h.regOk _ _ _ _ _ hreg).1, Nat.le_refl _⟩
      · have := h.memEpoch ev hm; rw [ho] at this; exact this
      · show σ.curGen o' g' ≤ _; rw [hg]; exact genMax_le hm _ _ ho hgg
  · intro src' o' g' n' mv' hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.regOk src' o' g' n' mv' hold
    · simp at hn
  · intro src' o' g' k r hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.reportOk src' o' g' k r hold
    · simp at hn

theorem BaseInv.addMember {W : World} {P : Protocol} {σ : State} (h : BaseInv W σ) {src o g n mv : Nat}
    (hreg : Msg.reg src o g n mv ∈ σ.msgs) : BaseInv W (σ.addMember W P src o g n mv) := by
  let x : MemEv := ⟨o, g, σ.openEpoch (W.home o), σ.nextGen P o g n, true⟩
  have hext : ∀ o g a, HasReg σ.mems o g a → HasReg (σ.mems ++ [x]) o g a := fun _ _ _ hr => hr.append _
  have hgm : ∀ o g, genMax σ.mems o g ≤ genMax (σ.mems ++ [x]) o g := fun o g => genMax_append_mono _ _ o g
  refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
           commitsOk := CommitsOk.record h hext _ [] (by simp), memEpoch := ?_, edgeOk := h.edgeOk,
           edgeSorted := h.edgeSorted, memSorted := ?_, markOk := ?_, replyOk := ?_, regOk := ?_, reportOk := ?_ }
  · intro ev hev
    rcases List.mem_append.mp hev with hold | hn
    · exact h.memEpoch ev hold
    · rw [List.mem_singleton] at hn; subst hn; exact Nat.le_refl _
  · exact pairwise_append_single h.memSorted (fun a ha hobj => by
      have := h.memEpoch a ha; simp only at hobj; rw [hobj] at this; exact this)
  · intro p o' g' mk hm
    obtain ⟨h1, h2, h3, h4⟩ := h.markOk p o' g' mk hm
    exact ⟨h1, hext _ _ _ h2, Nat.le_trans h3 (hgm _ _), h4⟩
  · intro dst o' g' gen c mv' hr
    rcases List.mem_append.mp hr with hold | hn
    · obtain ⟨h1, h2, h3, h4⟩ := h.replyOk dst o' g' gen c mv' hold
      exact ⟨hext _ _ _ h1, Nat.le_trans h2 (hgm _ _), h3, h4⟩
    · simp only [List.mem_singleton, Msg.reply.injEq] at hn
      obtain ⟨rfl, rfl, rfl, rfl, rfl, rfl, -⟩ := hn
      refine ⟨⟨x, List.mem_append_right _ (List.mem_singleton_self _), rfl, rfl, rfl, Nat.le_refl _⟩, ?_,
        (h.regOk _ _ _ _ _ hreg).1, Nat.le_refl _⟩
      exact genMax_le (List.mem_append_right _ (List.mem_singleton_self x)) _ _ rfl rfl
  · intro src' o' g' n' mv' hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.regOk src' o' g' n' mv' hold
    · simp at hn
  · intro src' o' g' k r hr
    rcases List.mem_append.mp hr with hold | hn
    · obtain ⟨h1, h2⟩ := h.reportOk src' o' g' k r hold
      exact ⟨Nat.le_trans h1 (hgm _ _), h2⟩
    · simp at hn

theorem BaseInv.accept {W : World} {P : Protocol} {σ : State} (h : BaseInv W σ) {src o g n mv : Nat}
    (hreg : Msg.reg src o g n mv ∈ σ.msgs) : BaseInv W (σ.accept W P src o g n mv) := by
  unfold State.accept
  split
  · exact h.acceptTomb src o g mv
  · split
    · exact h.acceptRepeat hreg (by assumption)
    · exact h.addMember hreg

theorem BaseInv.insertHome {W : World} {σ : State} (h : BaseInv W σ) {p s o : Nat}
    (hp : W.place (σ.openEpoch p) s = p) (hh : W.home o = p) (hand : Handover W σ.delivered s (σ.openEpoch p)) :
    BaseInv W (σ.insertHome W p s o) := by
  obtain ⟨hedge, hsort⟩ := h.newEdge true (o := o) hp hand
  unfold State.insertHome
  split
  · rename_i hmem
    obtain ⟨ev, hc, hpres⟩ := isMember_spec hmem
    obtain ⟨hm, ho, hg⟩ := memCur_mem hc
    refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
             commitsOk := CommitsOk.record h (fun _ _ _ hr => hr) _ _ ?_, memEpoch := h.memEpoch, edgeOk := hedge,
             edgeSorted := hsort, memSorted := h.memSorted, markOk := h.markOk, replyOk := h.replyOk, regOk := h.regOk,
             reportOk := h.reportOk }
    intro r hr; simp only [List.mem_singleton] at hr; subst hr
    refine ⟨Nat.le_refl _, ev, hm, ho, hg, hpres, ?_⟩
    have := h.memEpoch ev hm; rw [ho, hh] at this; exact this
  · let x : MemEv := ⟨o, W.grp s, σ.openEpoch p, genMax σ.mems o (W.grp s) + 1, true⟩
    have hext : ∀ o g a, HasReg σ.mems o g a → HasReg (σ.mems ++ [x]) o g a := fun _ _ _ hr => hr.append _
    have hgm : ∀ o g, genMax σ.mems o g ≤ genMax (σ.mems ++ [x]) o g := fun o g => genMax_append_mono _ _ o g
    refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
             commitsOk := CommitsOk.record h hext _ _ ?_, memEpoch := ?_, edgeOk := hedge, edgeSorted := hsort,
             memSorted := ?_, markOk := ?_, replyOk := ?_, regOk := h.regOk, reportOk := ?_ }
    · intro r hr; simp only [List.mem_singleton] at hr; subst hr
      exact ⟨Nat.le_refl _, x, List.mem_append_right _ (List.mem_singleton_self _), rfl, rfl, rfl, Nat.le_refl _⟩
    · intro ev hev
      rcases List.mem_append.mp hev with hold | hn
      · exact h.memEpoch ev hold
      · rw [List.mem_singleton] at hn; subst hn; show σ.openEpoch p ≤ _; rw [hh]; exact Nat.le_refl _
    · exact pairwise_append_single h.memSorted (fun a ha hobj => by
        have := h.memEpoch a ha; simp only at hobj; rw [hobj, hh] at this; exact this)
    · intro q o' g' mk hm
      obtain ⟨h1, h2, h3, h4⟩ := h.markOk q o' g' mk hm
      exact ⟨h1, hext _ _ _ h2, Nat.le_trans h3 (hgm _ _), h4⟩
    · intro dst o' g' gen c mv' hr
      obtain ⟨h1, h2, h3, h4⟩ := h.replyOk dst o' g' gen c mv' hr
      exact ⟨hext _ _ _ h1, Nat.le_trans h2 (hgm _ _), h3, h4⟩
    · intro src' o' g' k r hr
      obtain ⟨h1, h2⟩ := h.reportOk src' o' g' k r hr
      exact ⟨Nat.le_trans h1 (hgm _ _), h2⟩

theorem BaseInv.insertMark {W : World} {σ : State} (h : BaseInv W σ) {p s o : Nat} {mk : Mark}
    (hp : W.place (σ.openEpoch p) s = p) (hm : σ.marks p o (W.grp s) = some mk)
    (hand : Handover W σ.delivered s (σ.openEpoch p)) : BaseInv W (σ.insertMark W p s o mk) := by
  obtain ⟨hedge, hsort⟩ := h.newEdge true (o := o) hp hand
  obtain ⟨h1, h2, _⟩ := h.markOk p o (W.grp s) mk hm
  exact { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
          commitsOk := CommitsOk.record h (fun _ _ _ hr => hr) _ _ (by
      intro r hr; simp only [List.mem_singleton] at hr; subst hr; exact ⟨h1, h2⟩),
          memEpoch := h.memEpoch, edgeOk := hedge, edgeSorted := hsort, memSorted := h.memSorted, markOk := h.markOk,
          replyOk := h.replyOk, regOk := h.regOk, reportOk := h.reportOk }

theorem BaseInv.insertReply {W : World} {σ : State} (h : BaseInv W σ) {p s o gen c mv : Nat}
    (hp : W.place (σ.openEpoch p) s = p) (hh : W.home o ≠ p)
    (hr : Msg.reply p o (W.grp s) gen c mv false ∈ σ.msgs) (hc : c ≤ σ.delivered p)
    (hand : Handover W σ.delivered s (σ.openEpoch p)) : BaseInv W (σ.insertReply W p s o gen c) := by
  obtain ⟨hedge, hsort⟩ := h.newEdge true (o := o) hp hand
  obtain ⟨r1, r2, _, _⟩ := h.replyOk p o (W.grp s) gen c mv hr
  refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
           commitsOk := CommitsOk.record h (fun _ _ _ hr => hr) _ _ (by
      intro r hr; simp only [List.mem_singleton] at hr; subst hr
      exact ⟨by simp [State.openEpoch]; omega, r1⟩),
           memEpoch := h.memEpoch, edgeOk := hedge, edgeSorted := hsort, memSorted := h.memSorted, markOk := ?_,
           replyOk := ?_, regOk := ?_, reportOk := h.reportOk }
  · intro q o' g' mk hm
    by_cases hk : q = p ∧ o' = o ∧ g' = W.grp s
    · obtain ⟨rfl, rfl, rfl⟩ := hk
      change upd3 σ.marks q o' (W.grp s) _ q o' (W.grp s) = some mk at hm
      rw [upd3_self] at hm; cases hm
      exact ⟨show c + 1 ≤ σ.delivered q + 1 by omega, r1, r2, ⟨s, σ.openEpoch q, rfl, hp⟩, fun he => hh he.symm⟩
    · change upd3 σ.marks p o (W.grp s) _ q o' g' = some mk at hm
      rw [upd3_other _ _ hk] at hm
      exact h.markOk q o' g' mk hm
  · intro dst o' g' gen' c' mv' hr'
    obtain ⟨h1, h2, h3, h4⟩ := h.replyOk dst o' g' gen' c' mv' hr'
    exact ⟨h1, h2, Nat.le_trans h3 (mver_bump_ge _ _ _ _ _ _ _), h4⟩
  · intro src o' g' n mv' hr'
    obtain ⟨h1, h2⟩ := h.regOk src o' g' n mv' hr'
    by_cases hk : src = p ∧ o' = o ∧ g' = W.grp s
    · obtain ⟨rfl, rfl, rfl⟩ := hk
      refine ⟨Nat.le_trans h1 (mver_bump_ge _ _ _ _ _ _ _), fun he => ?_⟩
      change mv' = upd3 σ.mver src o' (W.grp s) _ src o' (W.grp s) at he
      rw [upd3_self] at he; omega
    · refine ⟨?_, fun he => ?_⟩
      · show mv' ≤ upd3 σ.mver p o (W.grp s) _ src o' g'; rw [upd3_other _ _ hk]; exact h1
      · change mv' = upd3 σ.mver p o (W.grp s) _ src o' g' at he
        rw [upd3_other _ _ hk] at he
        show n = upd3 σ.seen p o (W.grp s) gen src o' g'; rw [upd3_other _ _ hk]; exact h2 he

theorem BaseInv.delete {W : World} {σ : State} (h : BaseInv W σ) {p s o : Nat}
    (hp : W.place (σ.openEpoch p) s = p) (hand : Handover W σ.delivered s (σ.openEpoch p)) :
    BaseInv W (σ.delete p s o) := by
  obtain ⟨hedge, hsort⟩ := h.newEdge false (o := o) hp hand
  exact { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
          commitsOk := CommitsOk.record h (fun _ _ _ hr => hr) _ _ (by simp), memEpoch := h.memEpoch, edgeOk := hedge,
          edgeSorted := hsort, memSorted := h.memSorted, markOk := h.markOk, replyOk := h.replyOk, regOk := h.regOk,
          reportOk := h.reportOk }

theorem BaseInv.report {W : World} {σ : State} (h : BaseInv W σ) {p o g : Nat} {mk : Mark}
    (hm : σ.marks p o g = some mk) : BaseInv W (σ.report p o g mk) := by
  obtain ⟨_, _, m3, m4, m5⟩ := h.markOk p o g mk hm
  refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
           commitsOk := CommitsOk.record h (fun _ _ _ hr => hr) _ _ (by simp), memEpoch := h.memEpoch, edgeOk := h.edgeOk,
           edgeSorted := h.edgeSorted, memSorted := h.memSorted, markOk := ?_, replyOk := ?_, regOk := ?_,
           reportOk := ?_ }
  · intro q o' g' mk' hm'
    by_cases hk : q = p ∧ o' = o ∧ g' = g
    · obtain ⟨rfl, rfl, rfl⟩ := hk
      change upd3 σ.marks q o' g' none q o' g' = some mk' at hm'
      rw [upd3_self] at hm'; cases hm'
    · change upd3 σ.marks p o g none q o' g' = some mk' at hm'
      rw [upd3_other _ _ hk] at hm'
      exact h.markOk q o' g' mk' hm'
  · intro dst o' g' gen c mv hr
    rcases List.mem_append.mp hr with hold | hn
    · obtain ⟨h1, h2, h3, h4⟩ := h.replyOk dst o' g' gen c mv hold
      exact ⟨h1, h2, Nat.le_trans h3 (mver_bump_ge _ _ _ _ _ _ _), h4⟩
    · simp at hn
  · intro src o' g' n mv hr
    rcases List.mem_append.mp hr with hold | hn
    · obtain ⟨h1, h2⟩ := h.regOk src o' g' n mv hold
      by_cases hk : src = p ∧ o' = o ∧ g' = g
      · obtain ⟨rfl, rfl, rfl⟩ := hk
        refine ⟨Nat.le_trans h1 (mver_bump_ge _ _ _ _ _ _ _), fun he => ?_⟩
        change mv = upd3 σ.mver src o' g' _ src o' g' at he
        rw [upd3_self] at he; omega
      · refine ⟨?_, fun he => ?_⟩
        · show mv ≤ upd3 σ.mver p o g _ src o' g'; rw [upd3_other _ _ hk]; exact h1
        · change mv = upd3 σ.mver p o g _ src o' g' at he
          rw [upd3_other _ _ hk] at he
          show n = upd3 σ.seen p o g mk.gen src o' g'; rw [upd3_other _ _ hk]; exact h2 he
    · simp at hn
  · intro src o' g' k r hr
    rcases List.mem_append.mp hr with hold | hn
    · exact h.reportOk src o' g' k r hold
    · simp only [List.mem_singleton, Msg.report.injEq] at hn
      obtain ⟨rfl, rfl, rfl, rfl, rfl⟩ := hn
      exact ⟨m3, m4, m5⟩

theorem BaseInv.removal {W : World} {σ : State} (h : BaseInv W σ) (o g k : Nat) :
    BaseInv W { σ with
      mems := σ.mems ++ [⟨o, g, σ.openEpoch (W.home o), k, false⟩]
      commits := σ.record (W.home o) [] } := by
  let x : MemEv := ⟨o, g, σ.openEpoch (W.home o), k, false⟩
  have hext : ∀ o g a, HasReg σ.mems o g a → HasReg (σ.mems ++ [x]) o g a := fun _ _ _ hr => hr.append _
  have hgm : ∀ o g, genMax σ.mems o g ≤ genMax (σ.mems ++ [x]) o g := fun o g => genMax_append_mono _ _ o g
  refine { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
           commitsOk := CommitsOk.record h hext _ [] (by simp), memEpoch := ?_, edgeOk := h.edgeOk,
           edgeSorted := h.edgeSorted, memSorted := ?_, markOk := ?_, replyOk := ?_, regOk := h.regOk, reportOk := ?_ }
  · intro ev hev
    rcases List.mem_append.mp hev with hold | hn
    · exact h.memEpoch ev hold
    · rw [List.mem_singleton] at hn; subst hn; exact Nat.le_refl _
  · exact pairwise_append_single h.memSorted (fun a ha hobj => by
      have := h.memEpoch a ha; simp only at hobj; rw [hobj] at this; exact this)
  · intro p o' g' mk hm
    obtain ⟨h1, h2, h3, h4⟩ := h.markOk p o' g' mk hm
    exact ⟨h1, hext _ _ _ h2, Nat.le_trans h3 (hgm _ _), h4⟩
  · intro dst o' g' gen c mv' hr
    obtain ⟨h1, h2, h3, h4⟩ := h.replyOk dst o' g' gen c mv' hr
    exact ⟨hext _ _ _ h1, Nat.le_trans h2 (hgm _ _), h3, h4⟩
  · intro src' o' g' k' r hr
    obtain ⟨h1, h2⟩ := h.reportOk src' o' g' k' r hr
    exact ⟨Nat.le_trans h1 (hgm _ _), h2⟩

theorem BaseInv.onReport {W : World} {P : Protocol} {σ : State} (h : BaseInv W σ) (o g k r : Nat) :
    BaseInv W (σ.onReport W P o g k r) := by
  unfold State.onReport
  split
  · exact h.removal o g k
  · exact h

theorem BaseInv.tombstone {W : World} {σ : State} (h : BaseInv W σ) (o : Nat) : BaseInv W (σ.tombstone W o) :=
  { delivLe := h.delivLe, assignedLe := h.assignedLe, heldAll := h.heldAll,
    commitsOk := CommitsOk.record h (fun _ _ _ hr => hr) _ [] (by simp), memEpoch := h.memEpoch,
    edgeOk := h.edgeOk, edgeSorted := h.edgeSorted, memSorted := h.memSorted, markOk := h.markOk,
    replyOk := h.replyOk, regOk := h.regOk, reportOk := h.reportOk }

theorem BaseInv.step {W : World} {P : Protocol} {σ σ' : State} (h : BaseInv W σ) (hs : Step W P σ σ') :
    BaseInv W σ' := by
  cases hs with
  | propose v => exact h.propose v
  | close => exact h.close
  | deliver p hlt => exact h.deliver hlt
  | register p o g _ => exact h.register p o g
  | accept src o g n mv hreg => exact h.accept hreg
  | insertHome p s o hp hh hand => exact h.insertHome hp hh hand
  | insertMark p s o mk hp _ hm hand => exact h.insertMark hp hm hand
  | insertReply p s o gen c mv hp hh hr _ hc hand => exact h.insertReply hp hh hr hc hand
  | delete p s o hp hand => exact h.delete hp hand
  | report p o g mk hm _ _ => exact h.report hm
  | reclaim src o g k r _ => exact h.onReport o g k r
  | tombstone o => exact h.tombstone o
  | crash => exact h

theorem BaseInv.steps {W : World} {P : Protocol} {σ₁ σ₂ : State} (h : BaseInv W σ₁) (hs : Steps W P σ₁ σ₂) :
    BaseInv W σ₂ := by
  induction hs with
  | refl => exact h
  | tail _ hstep ih => exact ih.step hstep

/-- Every reachable state of every protocol variant satisfies `BaseInv`. -/
theorem Reachable.base {W : World} {P : Protocol} {σ : State} (h : Reachable W P σ) : BaseInv W σ :=
  (BaseInv.init W).steps h

end PlacementModel
