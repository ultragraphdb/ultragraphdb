/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import RoundModel.Basic

/-!
# Transactions, read sets and commit-time operations

A read-write transaction reaches the commit step as a `TxnRequest`: its session and request number, the read version
it executed at, its read set, the writes it built against its snapshot, and the commit-time operations it asks the
commit step to decide. A committed transaction is logged as a `Commit`: the request, its commit version, and its
*resolved* writes, the request's writes followed by the writes its operations produced, so the log holds decisions and
not inputs (D2) and replaying it re-decides nothing.

**The read set.** Point keys and key ranges, `lo ≤ k < hi`. The spec's kinds of read map onto them: a list read whole,
a tested statement, a dictionary key found absent and the namespace's catalog key are points; a scan is a range. A
transaction *covers* a key when a point or a range of its read set holds it.

**Commit-time operations modelled** (spec `transactions`, "Commit-time operations"):

* `upsertAndLink key uid props links` — the node holding the unique value `key`, created with `uid` and the creation
  properties `props` when no node holds it; each link key is written with the node's identity. Insert-if-absent is
  the same operation with no links (`Op.insertIfAbsent`).
* `compareAndSet key expected new` — refuses the whole transaction, reporting the value found, when `key` does not
  hold `expected`.
* `counterAdd key delta` — reads nothing of the transaction's snapshot; adds to the value held at commit.

An operation is decided against the state its transaction is serialised after, with the transaction's own writes
applied first, and each operation sees the writes of the operations before it.

**Simplifications.** A link is a write of the node's identity under the link's key, not an append to a posting list;
index maintenance, reverse lists and statistics that a changeset derives are not modelled. The transaction's execution
— its reads of its own writes, its lifetime limits — is not modelled: a request is the result of an execution.
-/

namespace RoundModel

/-- A key range scanned by a transaction: the keys `k` with `lo ≤ k < hi`. -/
structure KeyRange where
  lo : Key
  hi : Key
  deriving DecidableEq, Repr

/-- Whether the range holds `k`. -/
def KeyRange.contains (r : KeyRange) (k : Key) : Bool := decide (r.lo ≤ k) && decide (k < r.hi)

/-- What a read-write transaction read: point keys and scanned ranges. -/
structure ReadSet where
  points : List Key
  ranges : List KeyRange
  deriving DecidableEq, Repr

/-- Whether the read set holds `k`, as a point or inside a range. -/
def ReadSet.covers (rs : ReadSet) (k : Key) : Bool :=
  rs.points.contains k || rs.ranges.any (fun r => r.contains k)

/-- A commit-time operation, decided by the commit step in commit order. -/
inductive Op where
  /-- Upsert-and-link on a unique key: resolve or create the node, then write each link with its identity. -/
  | upsertAndLink (key : Key) (uid : Value) (props : List Write) (links : List Key)
  /-- Compare-and-set on a single-valued property. -/
  | compareAndSet (key : Key) (expected : Option Value) (new : Value)
  /-- Counter add: commutative, reads nothing of the snapshot. -/
  | counterAdd (key : Key) (delta : Nat)
  deriving Repr

/-- Insert-if-absent: upsert-and-link with no links. -/
def Op.insertIfAbsent (key : Key) (uid : Value) (props : List Write) : Op := .upsertAndLink key uid props []

/-- What an operation returns in the reply. -/
inductive OpResult where
  /-- The node's identity and whether this operation created it. -/
  | node (uid : Value) (created : Bool)
  /-- The compare-and-set held and wrote. -/
  | swapped
  /-- The counter's value after the add. -/
  | counter (value : Value)
  deriving DecidableEq, Repr

/-- Decide one operation against `s`: its writes and its result, or the value a failed compare-and-set found. -/
def Op.run (s : State) : Op → Except (Option Value) (List Write × OpResult)
  | .upsertAndLink key uid props links =>
    match s key with
    | none => .ok (props ++ links.map (fun l => (l, uid)) ++ [(key, uid)], .node uid true)
    | some u => .ok (links.map (fun l => (l, u)), .node u false)
  | .compareAndSet key expected new =>
    if s key = expected then .ok ([(key, new)], .swapped) else .error (s key)
  | .counterAdd key delta =>
    let v := (s key).getD 0 + delta
    .ok ([(key, v)], .counter v)

/-- Decide operations in order, each against the writes of those before it. -/
def runOps (s : State) : List Op → Except (Option Value) (List Write × List OpResult)
  | [] => .ok ([], [])
  | o :: os =>
    match o.run s with
    | .error f => .error f
    | .ok (ws, r) =>
      match runOps (applyWrites s ws) os with
      | .error f => .error f
      | .ok (ws', rs) => .ok (ws ++ ws', r :: rs)

/-- A read-write transaction's commit request. -/
structure TxnRequest where
  /-- The session the request is sent through. -/
  session : Nat
  /-- The request number: one above the session's last committed request. -/
  reqNo : Nat
  /-- The version of the snapshot it executed at: the watermark when it began. -/
  readVersion : Nat
  /-- What it read. -/
  reads : ReadSet
  /-- What it wrote against its snapshot. -/
  writes : List Write
  /-- The operations it asks the commit step to decide. -/
  ops : List Op
  deriving Repr

/-- A transaction's resolved writes and operation results against the state `s` it is serialised after: its own
writes, then its operations' writes; a failed compare-and-set refuses the whole transaction. -/
def resolve (s : State) (t : TxnRequest) : Except (Option Value) (List Write × List OpResult) :=
  match runOps (applyWrites s t.writes) t.ops with
  | .error f => .error f
  | .ok (ws, rs) => .ok (t.writes ++ ws, rs)

/-- A committed transaction as the log holds it. -/
structure Commit where
  txn : TxnRequest
  /-- Its commit version. -/
  version : Nat
  /-- Its resolved writes: the request's writes, then its operations' writes. -/
  writes : List Write
  /-- Its operations' results, returned in the reply. -/
  results : List OpResult
  deriving Repr

/-! ## The operations' own properties -/

/-- Upsert-and-link resolves to the node the key holds when one does, and creates it, with the creation properties,
only when none does: after its writes the key holds the identity it returned. -/
theorem upsert_result_held (s : State) (key uid : Value) (props : List Write) (links : List Key)
    (ws : List Write) (u : Value) (created : Bool)
    (hrun : (Op.upsertAndLink key uid props links).run s = .ok (ws, .node u created))
    (hlinks : ∀ l ∈ links, l ≠ key) :
    applyWrites s ws key = some u ∧ (created = true ↔ s key = none) := by
  simp only [Op.run] at hrun
  cases hs : s key with
  | none =>
    rw [hs] at hrun
    simp only [Except.ok.injEq, Prod.mk.injEq, OpResult.node.injEq] at hrun
    obtain ⟨hws, hu, hc⟩ := hrun
    subst hws hu hc
    refine ⟨?_, by simp⟩
    rw [applyWrites_append]
    simp [applyWrites, State.set]
  | some v =>
    rw [hs] at hrun
    simp only [Except.ok.injEq, Prod.mk.injEq, OpResult.node.injEq] at hrun
    obtain ⟨hws, hu, hc⟩ := hrun
    subst hws hu hc
    refine ⟨?_, by simp⟩
    rw [applyWrites_get_of_not_writes s _ key ?_, hs]
    intro w hw
    simp only [List.mem_map] at hw
    obtain ⟨l, hl, hlw⟩ := hw
    subst hlw
    exact hlinks l hl

/-- Two upserts of one unique key decided one after the other resolve to one node: the second finds the node the first
created and does not create another. The spec's "Concurrent insert-if-absent yields one node". -/
theorem upsert_twice_one_node (s : State) (key uid₁ uid₂ : Value) (props₁ props₂ : List Write)
    (links₁ links₂ : List Key) (ws₁ : List Write) (u₁ : Value) (c₁ : Bool)
    (h₁ : (Op.upsertAndLink key uid₁ props₁ links₁).run s = .ok (ws₁, .node u₁ c₁))
    (hlinks : ∀ l ∈ links₁, l ≠ key) :
    (Op.upsertAndLink key uid₂ props₂ links₂).run (applyWrites s ws₁) =
      .ok (links₂.map (fun l => (l, u₁)), .node u₁ false) := by
  have hheld := (upsert_result_held s key uid₁ props₁ links₁ ws₁ u₁ c₁ h₁ hlinks).1
  simp [Op.run, hheld]

/-- Counter adds commute: two adds to one counter, decided in either order, leave it with the same value. -/
theorem counterAdd_comm (s : State) (key : Key) (d₁ d₂ : Nat) (ws₁ ws₂ ws₁' ws₂' : List Write)
    (r₁ r₂ r₁' r₂' : OpResult)
    (h₁ : (Op.counterAdd key d₁).run s = .ok (ws₁, r₁))
    (h₂ : (Op.counterAdd key d₂).run (applyWrites s ws₁) = .ok (ws₂, r₂))
    (h₂' : (Op.counterAdd key d₂).run s = .ok (ws₂', r₂'))
    (h₁' : (Op.counterAdd key d₁).run (applyWrites s ws₂') = .ok (ws₁', r₁')) :
    applyWrites (applyWrites s ws₁) ws₂ key = applyWrites (applyWrites s ws₂') ws₁' key := by
  simp only [Op.run, Except.ok.injEq, Prod.mk.injEq] at h₁ h₂ h₂' h₁'
  obtain ⟨rfl, -⟩ := h₁
  obtain ⟨rfl, -⟩ := h₂'
  obtain ⟨rfl, -⟩ := h₂
  obtain ⟨rfl, -⟩ := h₁'
  simp [applyWrites, State.set, Nat.add_assoc, Nat.add_comm d₁ d₂]

/-- A compare-and-set whose expected value is not held refuses its whole transaction, reporting the value found. -/
theorem compareAndSet_refuses (s : State) (t : TxnRequest) (key : Key) (expected : Option Value) (new : Value)
    (hops : t.ops = [.compareAndSet key expected new])
    (hne : applyWrites s t.writes key ≠ expected) :
    resolve s t = .error (applyWrites s t.writes key) := by
  simp [resolve, hops, runOps, Op.run, hne]

end RoundModel
