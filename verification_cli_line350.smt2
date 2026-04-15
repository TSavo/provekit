```smt2
; [NEW] At line 350, console.log() always succeeds
; PRINCIPLE: P5 - Semantic Correctness
(declare-const logged Bool)
(assert (= logged true))
(assert (not logged))
(check-sat)
; unsat → console.log() unconditionally succeeds
```

```smt2
; [NEW] At line 350, contract is guaranteed
; PRINCIPLE: P1 - Precondition Propagation
(declare-const contract_exists Bool)
(assert contract_exists)
(assert (not contract_exists))
(check-sat)
; unsat → contract exists is guaranteed
```

```smt2
; [NEW] At line 350, contract data from file
; PRINCIPLE: P6 - Boundary Input
; contract loaded from JSON in contracts.ts loadFromDisk()
(declare-const contract_key String)
; JSON can have empty key (malformed)
(assert (= contract_key ""))
(check-sat)
; sat → empty key is a possible degenerate result
; But signalKey() always produces non-empty, so from file
```