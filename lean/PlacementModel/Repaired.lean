/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import PlacementModel.Base

/-!
# The invariant of the repaired protocol: no edge outlives its referrer-set member

`RepInv` holds of every reachable state of `Protocol.repaired`. Its last field, `complete`, is the property the
reverse-read and reclamation theorems rest on: whenever the edge from `s` to `o` is visible at an epoch, so is `s`'s
group in `R(o)`. The other fields are what keeps it inductive:

* a mark, and a reply its holder can still commit with, rest on a member that stays present from the registration's
  epoch on;
* every report comes from a partition holding every bucket of the group (`reportNeedsWholeGroup`);
* while the member stands at a report's generation, its reporter is *quiet*: it holds no mark, it saw that generation,
  no reply it can commit with exists, and no subject of the group has an edge to the entity at or after the report's
  epoch.

When the home applies a report, quietness and `removalAwaitsReportEpoch` give that no edge of the group is visible at or
after the removal's epoch. A registration the reporter sends after its report names the reported generation, so the
home raises the member above it (`generationsOnlyRise`), and the report goes stale before the reporter can commit an
edge again.
-/

namespace PlacementModel

/-- While member `g` of `R(o)` stands at generation `k`, the partition `src` that reported `k` at epoch `r` has no mark
and no reply to commit with, saw `k`, and no subject of `g` has an edge to `o` at or after `r`. -/
def Quiet (W : World) (σ : State) (src o g k r : Nat) : Prop :=
  σ.marks src o g = none ∧ σ.seen src o g = k ∧
    (∀ gen c mv, Msg.reply src o g gen c mv false ∈ σ.msgs → mv ≠ σ.mver src o g) ∧
    (∀ s, W.grp s = g → ∀ e, r ≤ e → edgeAt σ.edges s o e = false)

/-- What every reachable state of the repaired protocol satisfies, beyond `BaseInv`. -/
structure RepInv (W : World) (σ : State) : Prop where
  /-- Every report comes from a partition holding all of its group. -/
  reportWhole : ∀ src o g k r, Msg.report src o g k r ∈ σ.msgs → Whole W g src
  /-- A mark's member is present from the mark's registration epoch on. -/
  markFrom : ∀ p o g mk, σ.marks p o g = some mk → presentFrom σ.mems o g mk.regEpoch
  /-- A reply its receiver can still commit with keeps its member present from `c + 1` on. -/
  replyFrom : ∀ dst o g gen c mv, Msg.reply dst o g gen c mv false ∈ σ.msgs → mv = σ.mver dst o g →
    CanHold W g dst → presentFrom σ.mems o g (c + 1)
  /-- A report whose generation is current leaves its reporter quiet. -/
  reportQ : ∀ src o g k r, Msg.report src o g k r ∈ σ.msgs → σ.curIs o g k = true → Quiet W σ src o g k r
  /-- Every visible edge's group is a member of its entity's referrer set at that epoch. -/
  complete : ∀ s o e, edgeAt σ.edges s o e = true → memAt σ.mems o (W.grp s) e = true

/-! ## Lemmas -/

theorem whole_unique {W : World} {g p q : Nat} (hw : Whole W g p) (hc : CanHold W g q) : q = p := by
  obtain ⟨s, t, hg, hp⟩ := hc
  rw [← hp]; exact hw t s hg

theorem curIs_append_other {σ : State} {x : MemEv} {o g k : Nat} (h : ¬(x.obj = o ∧ x.grp = g)) :
    ({ σ with mems := σ.mems ++ [x] } : State).curIs o g k = σ.curIs o g k := by
  unfold State.curIs
  show (match memCur (σ.mems ++ [x]) o g with
    | some ev => ev.present && decide (ev.gen = k)
    | none => false) = _
  rw [memCur_append_single, if_neg h]; rfl

theorem curIs_append_self {σ : State} {x : MemEv} {k : Nat} :
    ({ σ with mems := σ.mems ++ [x] } : State).curIs x.obj x.grp k = (x.present && decide (x.gen = k)) := by
  unfold State.curIs
  show (match memCur (σ.mems ++ [x]) x.obj x.grp with
    | some ev => ev.present && decide (ev.gen = k)
    | none => false) = _
  rw [memCur_append_single, if_pos ⟨rfl, rfl⟩]

theorem edgeAt_append_le {l : List EdgeEv} {x : EdgeEv} (hx : x.present = false) {s o e : Nat}
    (h : edgeAt (l ++ [x]) s o e = true) : edgeAt l s o e = true := by
  rw [edgeAt_append_single] at h
  split at h
  · rw [hx] at h; cases h
  · exact h

/-- The steps that change neither events, marks, generations, mark versions nor live replies and reports. -/
theorem RepInv.frame {W : World} {σ σ' : State} (h : RepInv W σ) (he : σ'.edges = σ.edges) (hm : σ'.mems = σ.mems)
    (hmk : σ'.marks = σ.marks) (hs : σ'.seen = σ.seen) (hv : σ'.mver = σ.mver)
    (hrep : ∀ dst o g gen c mv, Msg.reply dst o g gen c mv false ∈ σ'.msgs → Msg.reply dst o g gen c mv false ∈ σ.msgs)
    (hrpt : ∀ src o g k r, Msg.report src o g k r ∈ σ'.msgs → Msg.report src o g k r ∈ σ.msgs) : RepInv W σ' := by
  have hcur : ∀ o g k, σ'.curIs o g k = σ.curIs o g k := by intro o g k; unfold State.curIs; rw [hm]
  refine ⟨fun src o g k r hr => h.reportWhole src o g k r (hrpt _ _ _ _ _ hr), ?_, ?_, ?_, ?_⟩
  · intro p o g mk hmk'; rw [hm]; rw [hmk] at hmk'; exact h.markFrom p o g mk hmk'
  · intro dst o g gen c mv hr hmv hc; rw [hm]; rw [hv] at hmv; exact h.replyFrom dst o g gen c mv (hrep _ _ _ _ _ _ hr) hmv hc
  · intro src o g k r hr hc
    rw [hcur] at hc
    obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src o g k r (hrpt _ _ _ _ _ hr) hc
    refine ⟨by rw [hmk]; exact q1, by rw [hs]; exact q2, fun gen c mv hr' => by
      rw [hv]; exact q3 gen c mv (hrep _ _ _ _ _ _ hr'), fun s hg e hre => by rw [he]; exact q4 s hg e hre⟩
  · intro s o e hed; rw [hm]; rw [he] at hed; exact h.complete s o e hed

theorem RepInv.init (W : World) : RepInv W State.init := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩
  · intro src o g k r hr; cases hr
  · intro p o g mk hm; cases hm
  · intro dst o g gen c mv hr; cases hr
  · intro src o g k r hr; cases hr
  · intro s o e he; simp [State.init, edgeAt, visible, lastWhere] at he

/-! ## Preservation -/

theorem RepInv.acceptRepeat {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {src o g n mv : Nat}
    (hreg : Msg.reg src o g n mv ∈ σ.msgs) (hrep : σ.isRepeat o g n = true) :
    RepInv W { σ with msgs := σ.msgs ++ [.reply src o g (σ.curGen o g) (σ.delivered (W.home o)) mv false] } := by
  obtain ⟨ev, hc, hp, hn, _⟩ := isRepeat_spec hrep
  obtain ⟨hm, ho, _⟩ := memCur_mem hc
  refine ⟨?_, h.markFrom, ?_, ?_, h.complete⟩
  · intro src' o' g' k r hr
    rcases List.mem_append.mp hr with hold | hnew
    · exact h.reportWhole src' o' g' k r hold
    · simp at hnew
  · intro dst o' g' gen c mv' hr hmv hcan
    rcases List.mem_append.mp hr with hold | hnew
    · exact h.replyFrom dst o' g' gen c mv' hold hmv hcan
    · simp only [List.mem_singleton, Msg.reply.injEq] at hnew
      obtain ⟨rfl, rfl, rfl, rfl, rfl, rfl, -⟩ := hnew
      have := hb.memEpoch ev hm; rw [ho] at this
      exact presentFrom_of_cur hc hp this
  · intro src' o' g' k r hr hcur
    rcases List.mem_append.mp hr with hold | hnew
    · obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src' o' g' k r hold hcur
      refine ⟨q1, q2, fun gen c mv' hr' => ?_, q4⟩
      rcases List.mem_append.mp hr' with hold' | hnew'
      · exact q3 gen c mv' hold'
      · simp only [List.mem_singleton, Msg.reply.injEq] at hnew'
        obtain ⟨rfl, rfl, rfl, -, -, rfl, -⟩ := hnew'
        intro hmv
        have hseen := (hb.regOk _ _ _ _ _ hreg).2 hmv
        obtain ⟨ev', hc', _, hk⟩ := curIs_spec hcur
        rw [hc] at hc'; cases hc'
        show False
        change σ.seen src' o' g' = k at q2
        omega
    · simp at hnew

theorem RepInv.addMember {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {src o g n mv : Nat} :
    RepInv W (σ.addMember W Protocol.repaired src o g n mv) := by
  let x : MemEv := ⟨o, g, σ.openEpoch (W.home o), σ.nextGen Protocol.repaired o g n, true⟩
  have hgen : σ.nextGen Protocol.repaired o g n = max n (genMax σ.mems o g) + 1 := rfl
  refine ⟨?_, fun p o' g' mk hm => presentFrom_append_present (x := x) rfl (h.markFrom p o' g' mk hm), ?_, ?_, ?_⟩
  · intro src' o' g' k r hr
    rcases List.mem_append.mp hr with hold | hnew
    · exact h.reportWhole src' o' g' k r hold
    · simp at hnew
  · intro dst o' g' gen c mv' hr hmv hcan
    rcases List.mem_append.mp hr with hold | hnew
    · exact presentFrom_append_present (x := x) rfl (h.replyFrom dst o' g' gen c mv' hold hmv hcan)
    · simp only [List.mem_singleton, Msg.reply.injEq] at hnew
      obtain ⟨rfl, rfl, rfl, rfl, rfl, rfl, -⟩ := hnew
      intro e he
      show memAt (σ.mems ++ [x]) o' g' e = true
      rw [memAt_append_single, if_pos ⟨rfl, rfl, he⟩]
  · intro src' o' g' k r hr hcur
    rcases List.mem_append.mp hr with hold | hnew
    · by_cases hk : x.obj = o' ∧ x.grp = g'
      · obtain ⟨hxo, hxg⟩ := hk
        have hle := (hb.reportOk src' o' g' k r hold).1
        change ({ σ with mems := σ.mems ++ [x] } : State).curIs o' g' k = true at hcur
        rw [← hxo, ← hxg, curIs_append_self] at hcur
        simp only [Bool.true_and, decide_eq_true_eq] at hcur
        rw [← hxo, ← hxg] at hle
        change max n (genMax σ.mems o g) + 1 = k at hcur
        change k ≤ genMax σ.mems o g at hle
        omega
      · change ({ σ with mems := σ.mems ++ [x] } : State).curIs o' g' k = true at hcur
        rw [curIs_append_other hk] at hcur
        obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src' o' g' k r hold hcur
        refine ⟨q1, q2, fun gen c mv' hr' => ?_, q4⟩
        rcases List.mem_append.mp hr' with hold' | hnew'
        · exact q3 gen c mv' hold'
        · simp only [List.mem_singleton, Msg.reply.injEq] at hnew'
          obtain ⟨-, rfl, rfl, -⟩ := hnew'
          exact absurd ⟨rfl, rfl⟩ hk
    · simp at hnew
  · intro s o' e he
    exact memAt_append_present (x := x) rfl (h.complete s o' e he)

theorem RepInv.accept {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {src o g n mv : Nat}
    (hreg : Msg.reg src o g n mv ∈ σ.msgs) : RepInv W (σ.accept W Protocol.repaired src o g n mv) := by
  unfold State.accept
  split
  · refine h.frame rfl rfl rfl rfl rfl ?_ ?_
    · intro dst o' g' gen c mv' hr
      rcases List.mem_append.mp hr with hold | hnew
      · exact hold
      · simp at hnew
    · intro src' o' g' k r hr
      rcases List.mem_append.mp hr with hold | hnew
      · exact hold
      · simp at hnew
  · split
    · exact h.acceptRepeat hb hreg (by assumption)
    · exact h.addMember hb

theorem RepInv.insertHome {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {p s o : Nat}
    (hp : W.place (σ.openEpoch p) s = p) (hh : W.home o = p) : RepInv W (σ.insertHome W p s o) := by
  let y : EdgeEv := ⟨s, o, σ.openEpoch p, p, true⟩
  -- No report of `s`'s group can be current-and-quiet with an edge from the home: the reporter holds the whole group,
  -- so it would be the home, which never reports.
  have hnoq : ∀ src k r, Msg.report src o (W.grp s) k r ∈ σ.msgs → False := by
    intro src k r hr
    have hw := h.reportWhole src o (W.grp s) k r hr
    have hne := (hb.reportOk src o (W.grp s) k r hr).2.2
    have := hw (σ.openEpoch p) s rfl
    rw [hp] at this; exact hne (by rw [← this, hh])
  have hedges : ∀ src o' g' k r, Msg.report src o' g' k r ∈ σ.msgs → ∀ s', W.grp s' = g' → ∀ e,
      edgeAt (σ.edges ++ [y]) s' o' e = edgeAt σ.edges s' o' e := by
    intro src o' g' k r hr s' hg' e
    rw [edgeAt_append_single]
    split
    · rename_i hy
      obtain ⟨hs', ho', _⟩ := hy
      change s = s' at hs'; change o = o' at ho'
      subst hs'; subst ho'; subst hg'
      exact (hnoq src k r hr).elim
    · rfl
  unfold State.insertHome
  split
  · rename_i hmem
    obtain ⟨ev, hc, hpres⟩ := isMember_spec hmem
    obtain ⟨hm, ho, _⟩ := memCur_mem hc
    refine ⟨h.reportWhole, h.markFrom, h.replyFrom, ?_, ?_⟩
    · intro src o' g' k r hr hcur
      obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src o' g' k r hr hcur
      exact ⟨q1, q2, q3, fun s' hg' e hre => by
        show edgeAt (σ.edges ++ [y]) s' o' e = false
        rw [hedges src o' g' k r hr s' hg' e]; exact q4 s' hg' e hre⟩
    · intro s' o' e he
      change edgeAt (σ.edges ++ [y]) s' o' e = true at he
      rw [edgeAt_append_single] at he
      split at he
      · rename_i hy
        obtain ⟨hs', ho', hle⟩ := hy
        change s = s' at hs'; change o = o' at ho'; change σ.openEpoch p ≤ e at hle
        subst hs'; subst ho'
        have := hb.memEpoch ev hm; rw [ho, hh] at this
        rw [memAt_of_cur hc (by simp only [State.openEpoch] at hle; omega)]; exact hpres
      · exact h.complete s' o' e he
  · let x : MemEv := ⟨o, W.grp s, σ.openEpoch p, genMax σ.mems o (W.grp s) + 1, true⟩
    refine ⟨h.reportWhole, fun q o' g' mk hm => presentFrom_append_present (x := x) rfl (h.markFrom q o' g' mk hm),
      fun dst o' g' gen c mv hr hmv hcan => presentFrom_append_present (x := x) rfl
        (h.replyFrom dst o' g' gen c mv hr hmv hcan), ?_, ?_⟩
    · intro src o' g' k r hr hcur
      by_cases hk : x.obj = o' ∧ x.grp = g'
      · obtain ⟨hxo, hxg⟩ := hk
        change o = o' at hxo; change W.grp s = g' at hxg
        subst hxo; subst hxg
        exact (hnoq src k r hr).elim
      · change ({ σ with mems := σ.mems ++ [x] } : State).curIs o' g' k = true at hcur
        rw [curIs_append_other hk] at hcur
        obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src o' g' k r hr hcur
        exact ⟨q1, q2, q3, fun s' hg' e hre => by
          show edgeAt (σ.edges ++ [y]) s' o' e = false
          rw [hedges src o' g' k r hr s' hg' e]; exact q4 s' hg' e hre⟩
    · intro s' o' e he
      change edgeAt (σ.edges ++ [y]) s' o' e = true at he
      show memAt (σ.mems ++ [x]) o' (W.grp s') e = true
      rw [edgeAt_append_single] at he
      split at he
      · rename_i hy
        obtain ⟨hs', ho', hle⟩ := hy
        change s = s' at hs'; change o = o' at ho'
        subst hs'; subst ho'
        rw [memAt_append_single, if_pos ⟨rfl, rfl, hle⟩]
      · exact memAt_append_present rfl (h.complete s' o' e he)

/-- A new edge to `o` from `p`'s subject `s`, written under a mark or reply of `p`, never meets a quiet reporter: the
reporter would be `p` itself, which a quiet reporter's missing mark and stale replies rule out. -/
theorem RepInv.newEdge_quiet {W : World} {σ : State} (h : RepInv W σ) {p s o : Nat} (hcan : CanHold W (W.grp s) p)
    (hnot : ∀ src k r, Msg.report src o (W.grp s) k r ∈ σ.msgs → σ.curIs o (W.grp s) k = true → src = p → False)
    (y : EdgeEv) (hy : y = ⟨s, o, σ.openEpoch p, p, true⟩) :
    ∀ src o' g' k r, Msg.report src o' g' k r ∈ σ.msgs → σ.curIs o' g' k = true → ∀ s', W.grp s' = g' → ∀ e,
      edgeAt (σ.edges ++ [y]) s' o' e = edgeAt σ.edges s' o' e := by
  intro src o' g' k r hr hcur s' hg' e
  rw [edgeAt_append_single]
  split
  · rename_i hm
    obtain ⟨hs', ho', _⟩ := hm
    subst hy
    change s = s' at hs'; change o = o' at ho'
    subst hs'; subst ho'; subst hg'
    exact (hnot src k r hr hcur (whole_unique (h.reportWhole _ _ _ _ _ hr) hcan).symm).elim
  · rfl

theorem RepInv.insertMark {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {p s o : Nat} {mk : Mark}
    (hm : σ.marks p o (W.grp s) = some mk) : RepInv W (σ.insertMark W p s o mk) := by
  let y : EdgeEv := ⟨s, o, σ.openEpoch p, p, true⟩
  obtain ⟨hreg, _, _, hcan, _⟩ := hb.markOk p o (W.grp s) mk hm
  have hframe := h.newEdge_quiet hcan (fun src k r hr hcur hsp => by
    subst hsp
    have := (h.reportQ src o (W.grp s) k r hr hcur).1
    rw [hm] at this; cases this) y rfl
  refine ⟨h.reportWhole, h.markFrom, h.replyFrom, ?_, ?_⟩
  · intro src o' g' k r hr hcur
    obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src o' g' k r hr hcur
    exact ⟨q1, q2, q3, fun s' hg' e hre => by
      show edgeAt (σ.edges ++ [y]) s' o' e = false
      rw [hframe src o' g' k r hr hcur s' hg' e]; exact q4 s' hg' e hre⟩
  · intro s' o' e he
    change edgeAt (σ.edges ++ [y]) s' o' e = true at he
    rw [edgeAt_append_single] at he
    split at he
    · rename_i hy
      obtain ⟨hs', ho', hle⟩ := hy
      change s = s' at hs'; change o = o' at ho'; change σ.openEpoch p ≤ e at hle
      subst hs'; subst ho'
      exact h.markFrom p o (W.grp s) mk hm e (Nat.le_trans hreg hle)
    · exact h.complete s' o' e he

theorem RepInv.insertReply {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {p s o gen c mv : Nat}
    (hp : W.place (σ.openEpoch p) s = p) (hr : Msg.reply p o (W.grp s) gen c mv false ∈ σ.msgs)
    (hv : mv = σ.mver p o (W.grp s)) (hc : c ≤ σ.delivered p) : RepInv W (σ.insertReply W p s o gen c) := by
  let y : EdgeEv := ⟨s, o, σ.openEpoch p, p, true⟩
  have hcan : CanHold W (W.grp s) p := ⟨s, σ.openEpoch p, rfl, hp⟩
  have hfrom := h.replyFrom p o (W.grp s) gen c mv hr hv hcan
  have hframe := h.newEdge_quiet hcan (fun src k r hrp hcur hsp => by
    subst hsp
    exact (h.reportQ src o (W.grp s) k r hrp hcur).2.2.1 gen c mv hr hv) y rfl
  refine ⟨h.reportWhole, ?_, ?_, ?_, ?_⟩
  · intro q o' g' mk hm
    by_cases hk : q = p ∧ o' = o ∧ g' = W.grp s
    · obtain ⟨rfl, rfl, rfl⟩ := hk
      change upd3 σ.marks q o' (W.grp s) _ q o' (W.grp s) = some mk at hm
      rw [upd3_self] at hm; cases hm
      exact hfrom
    · change upd3 σ.marks p o (W.grp s) _ q o' g' = some mk at hm
      rw [upd3_other _ _ hk] at hm
      exact h.markFrom q o' g' mk hm
  · intro dst o' g' gen' c' mv' hr' hmv hcan'
    by_cases hk : dst = p ∧ o' = o ∧ g' = W.grp s
    · obtain ⟨rfl, rfl, rfl⟩ := hk
      change mv' = upd3 σ.mver dst o' (W.grp s) _ dst o' (W.grp s) at hmv
      rw [upd3_self] at hmv
      have := (hb.replyOk _ _ _ _ _ _ hr').2.2.1
      omega
    · change mv' = upd3 σ.mver p o (W.grp s) _ dst o' g' at hmv
      rw [upd3_other _ _ hk] at hmv
      exact h.replyFrom dst o' g' gen' c' mv' hr' hmv hcan'
  · intro src o' g' k r hrp hcur
    obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src o' g' k r hrp hcur
    have hk : ¬(src = p ∧ o' = o ∧ g' = W.grp s) := by
      rintro ⟨rfl, rfl, rfl⟩
      exact q3 gen c mv hr hv
    refine ⟨?_, ?_, fun gen' c' mv' hr' => ?_, fun s' hg' e hre => ?_⟩
    · show upd3 σ.marks p o (W.grp s) _ src o' g' = none; rw [upd3_other _ _ hk]; exact q1
    · show upd3 σ.seen p o (W.grp s) gen src o' g' = k; rw [upd3_other _ _ hk]; exact q2
    · show mv' ≠ upd3 σ.mver p o (W.grp s) _ src o' g'; rw [upd3_other _ _ hk]; exact q3 gen' c' mv' hr'
    · show edgeAt (σ.edges ++ [y]) s' o' e = false
      rw [hframe src o' g' k r hrp hcur s' hg' e]; exact q4 s' hg' e hre
  · intro s' o' e he
    change edgeAt (σ.edges ++ [y]) s' o' e = true at he
    rw [edgeAt_append_single] at he
    split at he
    · rename_i hy
      obtain ⟨hs', ho', hle⟩ := hy
      change s = s' at hs'; change o = o' at ho'; change σ.delivered p + 1 ≤ e at hle
      subst hs'; subst ho'
      exact hfrom e (by omega)
    · exact h.complete s' o' e he

theorem RepInv.delete {W : World} {σ : State} (h : RepInv W σ) (p s o : Nat) : RepInv W (σ.delete p s o) := by
  refine ⟨h.reportWhole, h.markFrom, h.replyFrom, ?_, ?_⟩
  · intro src o' g' k r hr hcur
    obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src o' g' k r hr hcur
    refine ⟨q1, q2, q3, fun s' hg' e hre => ?_⟩
    cases hq : edgeAt (σ.delete p s o).edges s' o' e with
    | false => rfl
    | true =>
      have := edgeAt_append_le rfl hq
      rw [q4 s' hg' e hre] at this; cases this
  · intro s' o' e he
    exact h.complete s' o' e (edgeAt_append_le rfl he)

theorem RepInv.report {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {p o g : Nat} {mk : Mark}
    (hm : σ.marks p o g = some mk)
    (hno : ∀ s, W.grp s = g → W.place (σ.openEpoch p) s = p → edgeCur σ.edges s o = false)
    (hw : Whole W g p) : RepInv W (σ.report p o g mk) := by
  refine ⟨?_, ?_, ?_, ?_, h.complete⟩
  · intro src o' g' k r hr
    rcases List.mem_append.mp hr with hold | hnew
    · exact h.reportWhole src o' g' k r hold
    · simp only [List.mem_singleton, Msg.report.injEq] at hnew
      obtain ⟨rfl, rfl, rfl, rfl, rfl⟩ := hnew
      exact hw
  · intro q o' g' mk' hm'
    by_cases hk : q = p ∧ o' = o ∧ g' = g
    · obtain ⟨rfl, rfl, rfl⟩ := hk
      change upd3 σ.marks q o' g' none q o' g' = some mk' at hm'
      rw [upd3_self] at hm'; cases hm'
    · change upd3 σ.marks p o g none q o' g' = some mk' at hm'
      rw [upd3_other _ _ hk] at hm'
      exact h.markFrom q o' g' mk' hm'
  · intro dst o' g' gen c mv hr hmv hcan
    have hr0 : Msg.reply dst o' g' gen c mv false ∈ σ.msgs := by
      rcases List.mem_append.mp hr with hold | hnew
      · exact hold
      · simp at hnew
    by_cases hk : dst = p ∧ o' = o ∧ g' = g
    · obtain ⟨rfl, rfl, rfl⟩ := hk
      change mv = upd3 σ.mver dst o' g' _ dst o' g' at hmv
      rw [upd3_self] at hmv
      have := (hb.replyOk _ _ _ _ _ _ hr0).2.2.1
      omega
    · change mv = upd3 σ.mver p o g _ dst o' g' at hmv
      rw [upd3_other _ _ hk] at hmv
      exact h.replyFrom dst o' g' gen c mv hr0 hmv hcan
  · intro src o' g' k r hr hcur
    have hrep0 : ∀ gen c mv, Msg.reply src o' g' gen c mv false ∈ (σ.report p o g mk).msgs →
        Msg.reply src o' g' gen c mv false ∈ σ.msgs := by
      intro gen c mv hr'
      rcases List.mem_append.mp hr' with hold | hnew
      · exact hold
      · simp at hnew
    rcases List.mem_append.mp hr with hold | hnew
    · obtain ⟨q1, q2, q3, q4⟩ := h.reportQ src o' g' k r hold hcur
      have hk : ¬(src = p ∧ o' = o ∧ g' = g) := by
        rintro ⟨rfl, rfl, rfl⟩; rw [hm] at q1; cases q1
      refine ⟨?_, ?_, fun gen c mv hr' => ?_, q4⟩
      · show upd3 σ.marks p o g none src o' g' = none; rw [upd3_other _ _ hk]; exact q1
      · show upd3 σ.seen p o g mk.gen src o' g' = k; rw [upd3_other _ _ hk]; exact q2
      · show mv ≠ upd3 σ.mver p o g _ src o' g'; rw [upd3_other _ _ hk]; exact q3 gen c mv (hrep0 gen c mv hr')
    · simp only [List.mem_singleton, Msg.report.injEq] at hnew
      obtain ⟨rfl, rfl, rfl, rfl, rfl⟩ := hnew
      refine ⟨?_, ?_, fun gen c mv hr' => ?_, fun s hg e hre => ?_⟩
      · show upd3 σ.marks src o' g' none src o' g' = none; rw [upd3_self]
      · show upd3 σ.seen src o' g' mk.gen src o' g' = mk.gen; rw [upd3_self]
      · show mv ≠ upd3 σ.mver src o' g' _ src o' g'
        rw [upd3_self]
        have := (hb.replyOk _ _ _ _ _ _ (hrep0 gen c mv hr')).2.2.1
        omega
      · show edgeAt σ.edges s o' e = false
        have hps : W.place (σ.openEpoch src) s = src := hw _ s hg
        refine edgeAt_false_of_cur (hno s hg hps) (fun ev hev hsub _ => ?_) e hre
        obtain ⟨hpart, hst, _⟩ := hb.edgeOk ev hev
        rw [hsub, hw ev.stamp s hg] at hpart
        rw [hpart] at hst
        exact hst

theorem RepInv.removal {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {src o g k r : Nat}
    (hr : Msg.report src o g k r ∈ σ.msgs) (hcur : σ.curIs o g k = true) (hre : r ≤ σ.openEpoch (W.home o)) :
    RepInv W { σ with
      mems := σ.mems ++ [⟨o, g, σ.openEpoch (W.home o), k, false⟩]
      commits := σ.record (W.home o) [] } := by
  let x : MemEv := ⟨o, g, σ.openEpoch (W.home o), k, false⟩
  obtain ⟨q1, _, q3, q4⟩ := h.reportQ src o g k r hr hcur
  have hw := h.reportWhole src o g k r hr
  refine ⟨h.reportWhole, ?_, ?_, ?_, ?_⟩
  · intro q o' g' mk hm
    by_cases hk : x.obj = o' ∧ x.grp = g'
    · obtain ⟨hxo, hxg⟩ := hk
      change o = o' at hxo; change g = g' at hxg
      subst hxo; subst hxg
      have hq := whole_unique hw (hb.markOk q x.obj x.grp mk hm).2.2.2.1
      subst hq
      rw [q1] at hm; cases hm
    · intro e he
      show memAt (σ.mems ++ [x]) o' g' e = true
      rw [memAt_append_other hk]; exact h.markFrom q o' g' mk hm e he
  · intro dst o' g' gen c mv hr' hmv hcan
    by_cases hk : x.obj = o' ∧ x.grp = g'
    · obtain ⟨hxo, hxg⟩ := hk
      change o = o' at hxo; change g = g' at hxg
      subst hxo; subst hxg
      have hq := whole_unique hw hcan
      subst hq
      exact (q3 gen c mv hr' hmv).elim
    · intro e he
      show memAt (σ.mems ++ [x]) o' g' e = true
      rw [memAt_append_other hk]; exact h.replyFrom dst o' g' gen c mv hr' hmv hcan e he
  · intro src' o' g' k' r' hr' hcur'
    by_cases hk : x.obj = o' ∧ x.grp = g'
    · obtain ⟨hxo, hxg⟩ := hk
      change ({ σ with mems := σ.mems ++ [x] } : State).curIs o' g' k' = true at hcur'
      rw [← hxo, ← hxg, curIs_append_self] at hcur'
      cases hcur'
    · change ({ σ with mems := σ.mems ++ [x] } : State).curIs o' g' k' = true at hcur'
      rw [curIs_append_other hk] at hcur'
      exact h.reportQ src' o' g' k' r' hr' hcur'
  · intro s' o' e he
    show memAt (σ.mems ++ [x]) o' (W.grp s') e = true
    rw [memAt_append_single]
    split
    · rename_i hm
      obtain ⟨hxo, hxg, hle⟩ := hm
      change o = o' at hxo; change g = W.grp s' at hxg; change σ.openEpoch (W.home o) ≤ e at hle
      subst hxo
      rw [q4 s' hxg.symm e (Nat.le_trans hre hle)] at he
      cases he
    · exact h.complete s' o' e he

theorem RepInv.onReport {W : World} {σ : State} (hb : BaseInv W σ) (h : RepInv W σ) {src o g k r : Nat}
    (hr : Msg.report src o g k r ∈ σ.msgs) : RepInv W (σ.onReport W Protocol.repaired o g k r) := by
  unfold State.onReport
  split
  · rename_i hc
    simp only [Protocol.repaired, Bool.not_true, Bool.false_or, Bool.and_eq_true, decide_eq_true_eq] at hc
    exact h.removal hb hr hc.1 hc.2
  · exact h

theorem RepInv.step {W : World} {σ σ' : State} (hb : BaseInv W σ) (h : RepInv W σ)
    (hs : Step W Protocol.repaired σ σ') : RepInv W σ' := by
  cases hs with
  | propose v => exact h.frame rfl rfl rfl rfl rfl (fun _ _ _ _ _ _ hr => hr) (fun _ _ _ _ _ hr => hr)
  | close => exact h.frame rfl rfl rfl rfl rfl (fun _ _ _ _ _ _ hr => hr) (fun _ _ _ _ _ hr => hr)
  | deliver p _ => exact h.frame rfl rfl rfl rfl rfl (fun _ _ _ _ _ _ hr => hr) (fun _ _ _ _ _ hr => hr)
  | register p o g _ =>
    refine h.frame rfl rfl rfl rfl rfl ?_ ?_
    · intro dst o' g' gen c mv hr
      rcases List.mem_append.mp hr with hold | hnew
      · exact hold
      · simp at hnew
    · intro src o' g' k r hr
      rcases List.mem_append.mp hr with hold | hnew
      · exact hold
      · simp at hnew
  | accept src o g n mv hreg => exact h.accept hb hreg
  | insertHome p s o hp hh _ => exact h.insertHome hb hp hh
  | insertMark p s o mk _ _ hm _ => exact h.insertMark hb hm
  | insertReply p s o gen c mv hp _ hr hv hc _ => exact h.insertReply hb hp hr hv hc
  | delete p s o _ _ => exact h.delete p s o
  | report p o g mk hm hno hw => exact h.report hb hm hno (hw rfl)
  | reclaim src o g k r hr => exact h.onReport hb hr
  | tombstone o => exact h.frame rfl rfl rfl rfl rfl (fun _ _ _ _ _ _ hr => hr) (fun _ _ _ _ _ hr => hr)
  | crash => exact h

theorem RepInv.steps {W : World} {σ₁ σ₂ : State} (hb : BaseInv W σ₁) (h : RepInv W σ₁)
    (hs : Steps W Protocol.repaired σ₁ σ₂) : RepInv W σ₂ := by
  induction hs with
  | refl => exact h
  | tail hs' hstep ih => exact (ih).step (hb.steps hs') hstep

/-- Every reachable state of the repaired protocol satisfies `RepInv`. -/
theorem Reachable.rep {W : World} {σ : State} (h : Reachable W Protocol.repaired σ) : RepInv W σ :=
  (RepInv.init W).steps (BaseInv.init W) h

end PlacementModel
