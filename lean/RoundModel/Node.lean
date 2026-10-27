/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import RoundModel.Log

/-!
# The node: rounds, sync, apply, failed syncs and crashes

The node's state is the log, the length of its synced prefix, the store, and whether a failed sync has poisoned the
log. Its history is the log's commits; the commit step validates against all of it, synced or not, since round
`k + 1` is resolved while round `k` syncs.

A `Step` is one of:

* `round` — the commit step closes a round of requests, decides it against the history and appends it to the log.
  Every request began at the watermark: its read version is at or below the watermark now, since a transaction reads
  at the watermark when it begins and the watermark never decreases (`Node.watermark_mono`).
* `sync` — the sync of the next unsynced round completes.
* `syncFail` — a sync fails; the log is poisoned, and no later round, sync or apply happens until a restart.
* `apply` — the next synced round is applied to the store.
* `crash` — power is lost and the node restarts and recovers, in one step. The log keeps a prefix at least as long as
  its synced prefix (the **sync-durability assumption**, `SyncDurable`), and possibly rounds written but not yet
  synced; the store keeps any prefix of the rounds it had applied; recovery replays the kept log above the store.

A round's transactions are *acknowledged* once it is synced and applied: the acknowledged commits are those of the
store's applied prefix, which lies within the synced prefix, and the visibility watermark is the highest version among
them.

**Simplifications.** A crash and its recovery are one step. A sync that tears a round while reporting success breaks
the sync-durability assumption and is outside the model; the spec's recovery reports such a round by name. Read-only
transactions are not steps: one reads `Store.read` at the watermark when it begins and is never validated.
-/

namespace RoundModel

/-- The node's state. -/
structure Node where
  /-- Every round appended, in position order. -/
  log : List Round
  /-- The length of the log's synced prefix. -/
  synced : Nat
  /-- The store: derived from the log's applied prefix. -/
  store : Store
  /-- A sync failed: no round, sync or apply until a restart. -/
  poisoned : Bool

/-- A fresh node. -/
def Node.init : Node := { log := [], synced := 0, store := Store.empty, poisoned := false }

/-- The node's history: every commit of its log, in commit order. -/
def Node.history (s : Node) : List Commit := commitsOf s.log

/-- The acknowledged commits: those of the rounds both synced and applied. -/
def Node.acked (s : Node) : List Commit := commitsOf (s.log.take s.store.applied)

/-- The visibility watermark: the highest commit version acknowledged. -/
def Node.watermark (s : Node) : Nat := maxVersion s.acked

/-- **The sync-durability assumption.** A crash keeps at least the rounds whose sync completed: a completed sync is
durable. It is the premise of the `crash` step and the model's one assumption about storage. -/
def SyncDurable (s : Node) (kept : Nat) : Prop := s.synced ≤ kept

/-- Power loss, restart and recovery: the log keeps its first `kept` rounds, the store its first `fjallKept`, and
recovery replays the kept log above the store. -/
def Node.crash (s : Node) (fjallKept kept : Nat) : Node :=
  { log := s.log.take kept
    synced := kept
    store := recover (Store.ofRounds (s.log.take fjallKept)) (s.log.take kept)
    poisoned := false }

/-- One step of the node. -/
inductive Step : Node → Node → Prop where
  /-- Close a round of requests, decide it against the history and append it to the log. -/
  | round (s : Node) (clock : Nat) (reqs : List TxnRequest)
      (hp : s.poisoned = false) (hbegin : ∀ t ∈ reqs, t.readVersion ≤ s.watermark) :
      Step s { s with log := s.log ++ [⟨(runRound s.history clock reqs).1⟩] }
  /-- The sync of the next unsynced round completes. -/
  | sync (s : Node) (hp : s.poisoned = false) (hlt : s.synced < s.log.length) :
      Step s { s with synced := s.synced + 1 }
  /-- A sync fails and poisons the log. -/
  | syncFail (s : Node) : Step s { s with poisoned := true }
  /-- Apply the next synced round to the store. -/
  | apply (s : Node) (hp : s.poisoned = false) (hlt : s.store.applied < s.synced) :
      Step s { s with store := replay s.store ((s.log.take (s.store.applied + 1)).drop s.store.applied) }
  /-- Power loss, restart and recovery, under the sync-durability assumption. -/
  | crash (s : Node) (fjallKept kept : Nat) (hf : fjallKept ≤ s.store.applied)
      (hdur : SyncDurable s kept) (hk : kept ≤ s.log.length) :
      Step s (s.crash fjallKept kept)

/-- Any number of steps. -/
inductive Steps : Node → Node → Prop where
  | refl (s : Node) : Steps s s
  | tail {s₁ s₂ s₃ : Node} : Steps s₁ s₂ → Step s₂ s₃ → Steps s₁ s₃

/-- A state some run from a fresh node reaches, through any number of rounds, syncs, failures and crashes. -/
def Reachable (s : Node) : Prop := Steps Node.init s

/-- What holds of every reachable state. -/
structure Inv (s : Node) : Prop where
  /-- Only synced rounds are applied. -/
  appliedLeSynced : s.store.applied ≤ s.synced
  /-- Only appended rounds are synced. -/
  syncedLeLog : s.synced ≤ s.log.length
  /-- The store is the replay of the log's applied prefix. -/
  storeIsReplay : s.store = Store.ofRounds (s.log.take s.store.applied)
  /-- Every commit is admissible after the commits before it. -/
  valid : Valid s.history

/-! ## Prefixes of the log -/

theorem commitsOf_take_prefix (l : List Round) {a b : Nat} (h : a ≤ b) :
    commitsOf (l.take a) ++ commitsOf ((l.take b).drop a) = commitsOf (l.take b) := by
  rw [← commitsOf_append]
  have : l.take a = (l.take b).take a := by rw [List.take_take, Nat.min_eq_left h]
  rw [this, List.take_append_drop]

theorem acked_append_history (s : Node) :
    s.acked ++ commitsOf (s.log.drop s.store.applied) = s.history := by
  unfold Node.acked Node.history
  rw [← commitsOf_append, List.take_append_drop]

theorem watermark_le_history (s : Node) : s.watermark ≤ maxVersion s.history := by
  rw [← acked_append_history]; exact maxVersion_le_append_left _ _

theorem Inv.applied_le_length {s : Node} (h : Inv s) : s.store.applied ≤ s.log.length :=
  Nat.le_trans h.appliedLeSynced h.syncedLeLog

theorem apply_next_store (l : List Round) (a : Nat) :
    replay (Store.ofRounds (l.take a)) ((l.take (a + 1)).drop a) = Store.ofRounds (l.take (a + 1)) := by
  rw [replay_ofRounds]
  have : l.take a = (l.take (a + 1)).take a := by rw [List.take_take, Nat.min_eq_left (Nat.le_succ _)]
  conv => lhs; arg 1; arg 1; rw [this]
  rw [List.take_append_drop]

/-- Applying the next synced round gives the store of the log's applied prefix one round longer. -/
theorem Inv.apply_store {s : Node} (h : Inv s) :
    replay s.store ((s.log.take (s.store.applied + 1)).drop s.store.applied) =
      Store.ofRounds (s.log.take (s.store.applied + 1)) := by
  have e := congrArg (fun z => replay z ((s.log.take (s.store.applied + 1)).drop s.store.applied)) h.storeIsReplay
  simp only at e
  rw [e, apply_next_store]

/-- After a crash the store is the replay of the kept log. -/
theorem Inv.crash_store {s : Node} (h : Inv s) {fjallKept kept : Nat} (hf : fjallKept ≤ s.store.applied)
    (hdur : SyncDurable s kept) (hk : kept ≤ s.log.length) :
    (s.crash fjallKept kept).store = Store.ofRounds (s.log.take kept) ∧
      (s.crash fjallKept kept).store.applied = kept := by
  have hfk : fjallKept ≤ kept := Nat.le_trans hf (Nat.le_trans h.appliedLeSynced hdur)
  have htake : s.log.take fjallKept = (s.log.take kept).take fjallKept := by
    rw [List.take_take, Nat.min_eq_left hfk]
  have hlenk : (s.log.take kept).length = kept := by rw [List.length_take]; omega
  have hstore : (s.crash fjallKept kept).store = Store.ofRounds (s.log.take kept) := by
    show recover (Store.ofRounds (s.log.take fjallKept)) (s.log.take kept) = _
    rw [htake]; exact recover_prefix _ _ (by rw [hlenk]; exact hfk)
  refine ⟨hstore, ?_⟩
  rw [hstore]; simp only [Store.ofRounds, hlenk]

/-! ## The invariant holds of every reachable state -/

theorem Inv.init : Inv Node.init :=
  { appliedLeSynced := Nat.le_refl _
    syncedLeLog := Nat.le_refl _
    storeIsReplay := rfl
    valid := trivial }

theorem Inv.step {s s' : Node} (h : Inv s) (hs : Step s s') : Inv s' := by
  cases hs with
  | round clock reqs hp hbegin =>
    have hlen := h.applied_le_length
    refine
      { appliedLeSynced := h.appliedLeSynced
        syncedLeLog := by simp only [List.length_append]; exact Nat.le_trans h.syncedLeLog (Nat.le_add_right _ _)
        storeIsReplay := by simp only; rw [List.take_append_of_le_length hlen]; exact h.storeIsReplay
        valid := ?_ }
    show Valid (commitsOf (s.log ++ [⟨(runRound s.history clock reqs).1⟩]))
    rw [commitsOf_append]
    simp only [commitsOf, List.append_nil]
    refine (validFrom_append [] _ _).mpr ⟨h.valid, ?_⟩
    rw [List.nil_append]
    exact runRound_valid _ _ _ (valid_even h.valid)
      (fun t ht => Nat.le_trans (hbegin t ht) (watermark_le_history s))
  | sync hp hlt =>
    exact { appliedLeSynced := Nat.le_trans h.appliedLeSynced (Nat.le_succ _)
            syncedLeLog := hlt
            storeIsReplay := h.storeIsReplay
            valid := h.valid }
  | syncFail =>
    exact { appliedLeSynced := h.appliedLeSynced
            syncedLeLog := h.syncedLeLog
            storeIsReplay := h.storeIsReplay
            valid := h.valid }
  | apply hp hlt =>
    have hlen : s.store.applied + 1 ≤ s.log.length := Nat.le_trans hlt h.syncedLeLog
    have hstore := h.apply_store
    have happ : (Store.ofRounds (s.log.take (s.store.applied + 1))).applied = s.store.applied + 1 := by
      simp only [Store.ofRounds, List.length_take]; omega
    refine
      { appliedLeSynced := ?_
        syncedLeLog := h.syncedLeLog
        storeIsReplay := ?_
        valid := h.valid }
    · show (replay s.store _).applied ≤ s.synced
      rw [hstore, happ]; exact hlt
    · show replay s.store _ = Store.ofRounds (s.log.take (replay s.store _).applied)
      rw [hstore, happ]
  | crash fjallKept kept hf hdur hk =>
    obtain ⟨hstore, happ⟩ := h.crash_store hf hdur hk
    have hlenk : (s.log.take kept).length = kept := by rw [List.length_take]; omega
    refine
      { appliedLeSynced := ?_
        syncedLeLog := ?_
        storeIsReplay := ?_
        valid := ?_ }
    · show (s.crash fjallKept kept).store.applied ≤ kept
      rw [happ]; exact Nat.le_refl _
    · show kept ≤ (s.log.take kept).length
      rw [hlenk]; exact Nat.le_refl _
    · show (s.crash fjallKept kept).store = Store.ofRounds ((s.log.take kept).take (s.crash fjallKept kept).store.applied)
      rw [happ, hstore, List.take_take, Nat.min_self]
    · show Valid (commitsOf (s.log.take kept))
      have hv := h.valid
      unfold Node.history at hv
      rw [← List.take_append_drop kept s.log, commitsOf_append] at hv
      exact valid_prefix hv

theorem Inv.steps {s₁ s₂ : Node} (h : Inv s₁) (hs : Steps s₁ s₂) : Inv s₂ := by
  induction hs with
  | refl => exact h
  | tail _ hstep ih => exact ih.step hstep

/-- Every reachable state satisfies the invariant. -/
theorem Reachable.inv {s : Node} (h : Reachable s) : Inv s := Inv.init.steps h

theorem Reachable.steps {s₁ s₂ : Node} (h : Reachable s₁) (hs : Steps s₁ s₂) : Reachable s₂ := by
  induction hs with
  | refl => exact h
  | tail _ hstep ih => exact Steps.tail ih hstep

/-! ## Acknowledged commits only accumulate -/

theorem acked_prefix_of_le (l : List Round) {a b : Nat} (h : a ≤ b) :
    commitsOf (l.take a) <+: commitsOf (l.take b) :=
  ⟨_, commitsOf_take_prefix l h⟩

theorem Step.acked_prefix {s s' : Node} (h : Inv s) (hs : Step s s') : s.acked <+: s'.acked := by
  cases hs with
  | round clock reqs hp hbegin =>
    show commitsOf (s.log.take s.store.applied) <+: commitsOf ((s.log ++ _).take s.store.applied)
    rw [List.take_append_of_le_length h.applied_le_length]
    exact List.prefix_refl _
  | sync => exact List.prefix_refl _
  | syncFail => exact List.prefix_refl _
  | apply hp hlt =>
    show commitsOf (s.log.take s.store.applied) <+:
      commitsOf (s.log.take (replay s.store ((s.log.take (s.store.applied + 1)).drop s.store.applied)).applied)
    apply acked_prefix_of_le
    rw [h.apply_store]
    simp only [Store.ofRounds, List.length_take]
    have := Nat.le_trans hlt h.syncedLeLog
    omega
  | crash fjallKept kept hf hdur hk =>
    show commitsOf (s.log.take s.store.applied) <+:
      commitsOf ((s.log.take kept).take (s.crash fjallKept kept).store.applied)
    rw [(h.crash_store hf hdur hk).2, List.take_take, Nat.min_self]
    exact acked_prefix_of_le _ (Nat.le_trans h.appliedLeSynced hdur)

/-- Along any run, the acknowledged commits of an earlier state are a prefix of those of a later one, and the
invariant holds throughout. -/
theorem Steps.acked_prefix {s₁ s₂ : Node} (h : Inv s₁) (hs : Steps s₁ s₂) : s₁.acked <+: s₂.acked := by
  induction hs with
  | refl => exact List.prefix_refl _
  | tail hs' hstep ih => exact ih.trans (Step.acked_prefix (h.steps hs') hstep)

/-- The watermark never decreases, across crashes too. -/
theorem Node.watermark_mono {s₁ s₂ : Node} (h : Inv s₁) (hs : Steps s₁ s₂) : s₁.watermark ≤ s₂.watermark := by
  obtain ⟨t, ht⟩ := Steps.acked_prefix h hs
  unfold Node.watermark
  rw [← ht]
  exact maxVersion_le_append_left _ _

/-- The acknowledged commits are a prefix of the history. -/
theorem acked_prefix_history (s : Node) : s.acked <+: s.history := ⟨_, acked_append_history s⟩

end RoundModel
