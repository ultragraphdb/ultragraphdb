/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import RoundModel.Node

/-!
# The four properties

Each theorem holds of every reachable state, so for every number of transactions, rounds, failed syncs and crashes.
Each carries, under **Simulator invariant**, the check a trace of the node simulator (task 11.3) must pass after every
step and every recovery to restate it.

1. Strict serializability: `reads_are_serial`, `serializable`, `commit_order_is_version_order`, `real_time_order`,
   with `snapshot_stable` and `read_only_at_watermark` for reads.
2. Durability: `durability`, `crash_recovers_synced_log`, `failed_sync_acknowledges_nothing`.
3. Idempotent replay: `idempotent_replay`, with `applyAt_applied`, `replay_whole_log`, `recover_prefix`,
   `recover_idem` and `round_apply_idem` in `RoundModel.Log`.
4. Sessions: `session_commits_at_most_once`, `acknowledged_request_commits_once`, with `decideTxn_retry` in
   `RoundModel.Round`.
-/

namespace RoundModel

/-! ## Snapshots -/

theorem filter_le_nil {l : List Commit} {v : Nat} (h : ∀ d ∈ l, v < d.version) :
    l.filter (fun c => decide (c.version ≤ v)) = [] :=
  List.filter_eq_nil_iff.mpr (fun d hd => by have := h d hd; simp; omega)

theorem snapshot_append_above (A X : List Commit) (v : Nat) (h : ∀ d ∈ X, v < d.version) :
    snapshot (A ++ X) v = snapshot A v := by
  unfold snapshot
  rw [List.filter_append, filter_le_nil h, List.append_nil]

/-- **Snapshots are stable.** A read at or below the watermark gives, from the store, what the history of every later
state gives at that version: every commit acknowledged before the read is visible to it, and no commit becomes visible
to it later.

**Simulator invariant:** for every read at version `v` served at step `i`, with `v` at or below the watermark at `i`,
and every later step `j`, the read's result equals the multiversion read at `v` of the log at `j`. -/
theorem snapshot_stable {s₁ s₂ : Node} (h₁ : Reachable s₁) (hs : Steps s₁ s₂) {v : Nat}
    (hv : v ≤ s₁.watermark) (k : Key) : s₁.store.read v k = snapshot s₂.history v k := by
  have hinv₁ := h₁.inv
  have hinv₂ := (h₁.steps hs).inv
  obtain ⟨t, ht⟩ := Steps.acked_prefix hinv₁ hs
  obtain ⟨u, hu⟩ := acked_prefix_history s₂
  have hhist : s₂.history = s₁.acked ++ (t ++ u) := by rw [← hu, ← ht, List.append_assoc]
  have hvalid := hinv₂.valid
  rw [hhist] at hvalid
  have hfrom := ((validFrom_append [] s₁.acked (t ++ u)).mp hvalid).2
  rw [List.nil_append] at hfrom
  have habove := (validFrom_versions hfrom).2
  have hdata : s₁.store.data = s₁.acked := congrArg Store.data hinv₁.storeIsReplay
  unfold Store.read
  rw [hdata, hhist, snapshot_append_above _ _ _ (fun d hd => Nat.lt_of_le_of_lt hv (habove d hd))]

/-- **A read-only transaction reads a serial prefix.** A read at the watermark returns the state the acknowledged
commits produce in commit order: the serial state of a prefix of the commit order, holding every commit acknowledged
before the read began. With `snapshot_stable`, it stays that state in every later history, so a read-only
transaction, never validated, is serialized after every commit it saw and before every commit it did not.

**Simulator invariant:** every read at the watermark `W` returns, for each key, the value after replaying serially the
commits acknowledged when it began. -/
theorem read_only_at_watermark {s : Node} (hr : Reachable s) (k : Key) :
    s.store.read s.watermark k = serial s.acked k := by
  have hdata : s.store.data = s.acked := congrArg Store.data hr.inv.storeIsReplay
  unfold Store.read snapshot
  rw [hdata, List.filter_eq_self.mpr (fun d hd => by simp [Node.watermark]; exact le_maxVersion hd)]

/-! ## 1. Strict serializability -/

/-- **Serializable.** In the history of every reachable state, each committed transaction's reads at its read version
equal the state the commits before it produce in commit order, for every key its read set covers, points and ranges
alike; its logged writes and operation results are what it resolves to against that state; and its read version is
below its commit version. The history in commit order is therefore a serial history every commit agrees with.

**Simulator invariant:** for every commit `c` at index `i` of the log's commits, and every key `k` its read set
covers, the multiversion read of `k` at `c`'s read version equals the value of `k` after replaying commits `0 … i-1`
serially; replaying `c`'s request against that state yields exactly `c`'s logged writes and results. -/
theorem serializable {s : Node} (hr : Reachable s) {pre post : List Commit} {c : Commit}
    (hsplit : s.history = pre ++ c :: post) :
    (∀ k, c.txn.reads.covers k = true → snapshot s.history c.txn.readVersion k = serial pre k) ∧
      resolve (serial pre) c.txn = .ok (c.writes, c.results) ∧
      c.txn.readVersion < c.version := by
  have hvalid := hr.inv.valid
  rw [hsplit] at hvalid
  have hadm := valid_split hvalid
  have hlt : c.txn.readVersion < c.version := Nat.lt_of_le_of_lt hadm.readBelow hadm.versionAbove
  refine ⟨?_, hadm.resolved, hlt⟩
  intro k hk
  have hlater : ∀ d ∈ c :: post, c.txn.readVersion < d.version := by
    intro d hd
    cases List.mem_cons.mp hd with
    | inl he => rw [he]; exact hlt
    | inr ht => exact Nat.lt_trans hlt (valid_later_gt hvalid d ht)
  rw [hsplit, snapshot_append_above _ _ _ hlater]
  unfold snapshot serial
  apply serialFrom_filter_get _ k pre _ _ _ rfl
  intro c' hc' hp w hw hwk
  have hgt : c.txn.readVersion < c'.version := by simp at hp; omega
  have := hadm.noConflict c' hc' hgt w hw
  rw [hwk, hk] at this
  cases this

/-- **What a transaction read is serial.** A transaction that executed against the store of a state `s₀`, at a read
version at or below that state's watermark, and committed into the history of a later state `s` read, for every key
its read set covers, the value the commits before it produce in commit order. This combines `snapshot_stable` with
`serializable`: the read it made when it executed is the read the serial order gives it at its commit.

**Simulator invariant:** for every committed transaction, each value it read while executing equals the value of that
key after replaying, serially, the commits before it in the log. -/
theorem reads_are_serial {s₀ s : Node} (h₀ : Reachable s₀) (hs : Steps s₀ s) {pre post : List Commit} {c : Commit}
    (hrv : c.txn.readVersion ≤ s₀.watermark) (hsplit : s.history = pre ++ c :: post)
    (k : Key) (hk : c.txn.reads.covers k = true) :
    s₀.store.read c.txn.readVersion k = serial pre k := by
  rw [snapshot_stable h₀ hs hrv k]
  exact (serializable (h₀.steps hs) hsplit).1 k hk

/-- **Commit order is version order.** Along the history of every reachable state, commit versions strictly increase
and are even.

**Simulator invariant:** the commit versions of the log's commits, in log order, are even and strictly increasing,
across recoveries too. -/
theorem commit_order_is_version_order {s : Node} (hr : Reachable s) :
    s.history.Pairwise (fun a b => a.version < b.version) ∧ ∀ c ∈ s.history, c.version % 2 = 0 :=
  ⟨valid_versions hr.inv.valid, valid_even hr.inv.valid⟩

/-- **Real-time order.** A commit `A` acknowledged in a state `s₁` precedes, in the commit order of every later state,
every transaction `B` that began after it: one whose read version is at or above `A`'s commit version. A transaction
beginning at the watermark of any state after `s₁` has such a read version (`began_after_ack`).

**Simulator invariant:** for every commit `A` acknowledged at step `i` and every commit `B` whose transaction began at
a step after `i`, `A` precedes `B` in the log. -/
theorem real_time_order {s₁ s₂ : Node} (h₁ : Reachable s₁) (hs : Steps s₁ s₂) {A B : Commit}
    (hA : A ∈ s₁.acked) {pre post : List Commit} (hsplit : s₂.history = pre ++ B :: post)
    (hbegan : A.version ≤ B.txn.readVersion) : A ∈ pre := by
  have hr₂ := h₁.steps hs
  have hAin : A ∈ s₂.history := by
    obtain ⟨t, ht⟩ := Steps.acked_prefix h₁.inv hs
    obtain ⟨u, hu⟩ := acked_prefix_history s₂
    rw [← hu, ← ht]
    exact List.mem_append_left _ (List.mem_append_left _ hA)
  have hBlt := (serializable hr₂ hsplit).2.2
  have hvalid := hr₂.inv.valid
  rw [hsplit] at hvalid
  rw [hsplit] at hAin
  cases List.mem_append.mp hAin with
  | inl h => exact h
  | inr h =>
    cases List.mem_cons.mp h with
    | inl he => rw [he] at hbegan; omega
    | inr ht => have := valid_later_gt hvalid A ht; omega

/-- A transaction that begins at the watermark of a state after `A` was acknowledged has a read version at or above
`A`'s commit version: the premise of `real_time_order`. -/
theorem began_after_ack {s₁ s₂ : Node} (h₁ : Reachable s₁) (hs : Steps s₁ s₂) {A : Commit} (hA : A ∈ s₁.acked) :
    A.version ≤ s₂.watermark :=
  Nat.le_trans (le_maxVersion hA) (Node.watermark_mono h₁.inv hs)

/-! ## 2. Durability -/

theorem filter_le_split {pre post : List Commit} {A : Commit} (hv : Valid (pre ++ A :: post)) :
    (pre ++ A :: post).filter (fun c => decide (c.version ≤ A.version)) = pre ++ [A] := by
  have hadm := valid_split hv
  rw [List.filter_append, List.filter_cons]
  have hpre : pre.filter (fun c => decide (c.version ≤ A.version)) = pre :=
    List.filter_eq_self.mpr (fun d hd => by
      have := Nat.lt_of_le_of_lt (le_maxVersion hd) hadm.versionAbove
      simp; omega)
  rw [hpre, if_pos (by simp), filter_le_nil (valid_later_gt hv)]

/-- **Durability.** Every commit acknowledged in a state `s₁` is, in every later state — after any number of rounds,
failed syncs and crashes — still acknowledged and in the store, and the store read at its commit version returns each
of its writes.

Under the sync-durability assumption (`SyncDurable`, the `crash` step's premise): a crash keeps at least the synced
prefix of the log.

**Simulator invariant:** after every step and every recovery, every commit acknowledged so far is in the store, and a
read at its commit version of each key it wrote returns the value it wrote. -/
theorem durability {s₁ s₂ : Node} (h₁ : Reachable s₁) (hs : Steps s₁ s₂) {A : Commit} (hA : A ∈ s₁.acked) :
    A ∈ s₂.acked ∧ A ∈ s₂.store.data ∧
      ∀ k v, lookupWrite A.writes k = some v → s₂.store.read A.version k = some v := by
  have hr₂ := h₁.steps hs
  have hinv₂ := hr₂.inv
  obtain ⟨t, ht⟩ := Steps.acked_prefix h₁.inv hs
  have hA₂ : A ∈ s₂.acked := by rw [← ht]; exact List.mem_append_left _ hA
  have hdata : s₂.store.data = s₂.acked := congrArg Store.data hinv₂.storeIsReplay
  refine ⟨hA₂, hdata ▸ hA₂, ?_⟩
  intro k v hkv
  obtain ⟨pre, post, hsplit⟩ := List.append_of_mem hA₂
  have hvalid : Valid s₂.acked := by
    have hv := hinv₂.valid
    rw [← acked_append_history] at hv
    exact valid_prefix hv
  rw [hsplit] at hvalid
  unfold Store.read snapshot
  rw [hdata, hsplit, filter_le_split hvalid, serial_append_single, applyWrites_get, hkv]
  rfl

/-- **A crash recovers the synced log.** Recovery leaves the log at its kept prefix, which holds the synced prefix
(`SyncDurable`), and the store equal to the replay of that log, whatever prefix of its rounds the store kept.

**Simulator invariant:** after every recovery, the store equals the replay of the recovered log, and the recovered log
holds every round synced before the power loss. -/
theorem crash_recovers_synced_log {s : Node} (hr : Reachable s) {fjallKept kept : Nat}
    (hf : fjallKept ≤ s.store.applied) (hdur : SyncDurable s kept) (hk : kept ≤ s.log.length) :
    (s.crash fjallKept kept).store = Store.ofRounds (s.crash fjallKept kept).log ∧
      s.log.take s.synced <+: (s.crash fjallKept kept).log := by
  refine ⟨(hr.inv.crash_store hf hdur hk).1, ?_⟩
  show s.log.take s.synced <+: s.log.take kept
  refine ⟨(s.log.take kept).drop s.synced, ?_⟩
  have : s.log.take s.synced = (s.log.take kept).take s.synced := by rw [List.take_take, Nat.min_eq_left hdur]
  rw [this, List.take_append_drop]

/-- **A failed sync acknowledges nothing.** From a poisoned state, every step that does not restart the node leaves
the history and the acknowledged commits as they were: no round is decided, synced or applied until a restart.

**Simulator invariant:** after a failed sync and until the next recovery, no commit is acknowledged and no round is
appended. -/
theorem failed_sync_acknowledges_nothing {s s' : Node} (hp : s.poisoned = true) (hs : Step s s')
    (hp' : s'.poisoned = true) : s'.history = s.history ∧ s'.acked = s.acked := by
  cases hs with
  | round _ _ h => rw [hp] at h; cases h
  | sync h => rw [hp] at h; cases h
  | syncFail => exact ⟨rfl, rfl⟩
  | apply h => rw [hp] at h; cases h
  | crash => simp [Node.crash] at hp'

/-! ## 3. Idempotent replay -/

/-- **Idempotent replay.** For the log of every reachable state: replaying a round already applied leaves the store
unchanged; replaying the whole log, every round through its position, onto the store of any of its prefixes gives the
store of the log applied once; recovery from the store of any prefix of a synced prefix equals applying that synced
prefix once, and recovering again changes nothing; and the store equals the replay of the log's applied prefix from
an empty store.

**Simulator invariant:** after every step and every recovery, the store equals the replay of the log's applied prefix
from an empty store; replaying any round at or below the store's position, or recovering a second time, leaves the
store unchanged. -/
theorem idempotent_replay {s : Node} (hr : Reachable s) :
    (∀ (pos : Nat) (r : Round), pos ≤ s.store.applied → s.store.applyAt pos r = s.store) ∧
      (∀ j ≤ s.log.length, replayAt (Store.ofRounds (s.log.take j)) 1 s.log = Store.ofRounds s.log) ∧
      (∀ j n, j ≤ n → n ≤ s.synced →
        recover (Store.ofRounds (s.log.take j)) (s.log.take n) = Store.ofRounds (s.log.take n) ∧
        recover (recover (Store.ofRounds (s.log.take j)) (s.log.take n)) (s.log.take n) =
          Store.ofRounds (s.log.take n)) ∧
      s.store = replay Store.empty (s.log.take s.store.applied) := by
  have hinv := hr.inv
  refine ⟨fun pos r h => applyAt_applied _ _ _ h, fun j hj => replay_whole_log _ j hj, ?_, ?_⟩
  · intro j n hjn hn
    have hlen : (s.log.take n).length = n := by
      rw [List.length_take]; have := hinv.syncedLeLog; omega
    have htake : s.log.take j = (s.log.take n).take j := by rw [List.take_take, Nat.min_eq_left hjn]
    rw [htake]
    have h1 := recover_prefix (s.log.take n) j (by rw [hlen]; exact hjn)
    refine ⟨h1, ?_⟩
    rw [recover_idem _ j (by rw [hlen]; exact hjn), h1]
  · rw [Store.empty, replay_ofRounds, List.nil_append]
    exact hinv.storeIsReplay

/-! ## 4. Sessions -/

theorem filter_eq_nil_of_false {l : List Commit} {p : Commit → Bool} (h : ∀ d ∈ l, p d = false) :
    l.filter p = [] :=
  List.filter_eq_nil_iff.mpr (fun d hd => by simp [h d hd])

/-- **A session's request commits at most once.** In the history of every reachable state — across every retry and
every crash — at most one commit carries a given session and request number; within a session, request numbers
strictly increase in commit order.

**Simulator invariant:** after every step and every recovery, no two commits of the log carry the same session and
request number, and each session's request numbers increase along the log. -/
theorem session_commits_at_most_once {s : Node} (hr : Reachable s) (sid n : Nat) :
    (s.history.filter (fun c => decide (c.txn.session = sid) && decide (c.txn.reqNo = n))).length ≤ 1 ∧
      s.history.Pairwise (fun a b => a.txn.session = b.txn.session → a.txn.reqNo < b.txn.reqNo) := by
  have hpw := (validFrom_sessions hr.inv.valid).1
  refine ⟨?_, hpw⟩
  generalize s.history = H at hpw
  induction H with
  | nil => simp
  | cons c cs ih =>
    obtain ⟨hhead, htail⟩ := List.pairwise_cons.mp hpw
    rw [List.filter_cons]
    split
    · rename_i hc
      simp only [Bool.and_eq_true, decide_eq_true_eq] at hc
      rw [filter_eq_nil_of_false]
      · simp
      · intro d hd
        have hlt := hhead d hd
        simp only [Bool.and_eq_false_iff, decide_eq_false_iff_not]
        by_cases hs : d.txn.session = sid
        · right; have := hlt (by rw [hc.1, hs]); omega
        · left; exact hs
    · exact ih htail

/-- **An acknowledged request is committed exactly once from then on.** Once a commit is acknowledged, the history of
every later state holds exactly that one commit under its session and request number: neither a retry nor a crash
commits it again.

**Simulator invariant:** for every commit acknowledged at step `i`, the log at every later step holds exactly one
commit with its session and request number, and it is that commit. -/
theorem acknowledged_request_commits_once {s₁ s₂ : Node} (h₁ : Reachable s₁) (hs : Steps s₁ s₂) {A : Commit}
    (hA : A ∈ s₁.acked) :
    s₂.history.filter (fun c => decide (c.txn.session = A.txn.session) && decide (c.txn.reqNo = A.txn.reqNo)) =
      [A] := by
  have hr₂ := h₁.steps hs
  have hmem : A ∈ s₂.history := by
    obtain ⟨u, hu⟩ := acked_prefix_history s₂
    rw [← hu]; exact List.mem_append_left _ (durability h₁ hs hA).1
  have hlen := (session_commits_at_most_once hr₂ A.txn.session A.txn.reqNo).1
  have hin : A ∈ s₂.history.filter
      (fun c => decide (c.txn.session = A.txn.session) && decide (c.txn.reqNo = A.txn.reqNo)) :=
    List.mem_filter.mpr ⟨hmem, by simp⟩
  generalize s₂.history.filter _ = l at hlen hin
  match l, hlen, hin with
  | [x], _, hx => rw [List.mem_singleton.mp hx]
  | _ :: _ :: _, hl, _ => simp at hl

end RoundModel
