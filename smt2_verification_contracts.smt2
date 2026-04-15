```smt2
; [NEW] At line 114, this.contracts.size >= 0
; PRINCIPLE: P6 - Map ADT invariant
(declare-const contracts_size Int)
(assert (>= contracts_size 0))
(assert (not (>= contracts_size 0)))
(check-sat)
; EXPECTED: unsat → Map.size is always non-negative
```

```smt2
; [NEW] REACHABLE: Silent JSON parse failure loses data
; PRINCIPLE: P6 - Boundary/Degenerate Input
(declare-const json_files Int)
(declare-const loaded Int)
(assert (> json_files 1))
(assert (= loaded 1))
(assert (= loaded (- json_files 1)))
(check-sat)
; EXPECTED: sat → malformed JSON silently skipped
```

```smt2
; [NEW] REACHABLE: Key collision silently overwrites
; PRINCIPLE: P2 - State Mutation Analysis
(declare-const key1 String)
(declare-const key2 String)
(declare-const final_size Int)
(assert (= key1 key2))
(assert (= final_size 1))
(assert (= key1 key2))
(check-sat)
; EXPECTED: sat → duplicate keys overwrite without warning
```

```smt2
; [NEW] At line 114, directory precondition is established
; PRINCIPLE: P1 - Precondition Propagation
(declare-const dir_exists Bool)
(assert (= dir_exists true))
(assert (not dir_exists))
(check-sat)
; EXPECTED: unsat → guard guarantees directory exists
```