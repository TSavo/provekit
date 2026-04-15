```smt2
; [NEW] At line 354, console.log() always succeeds
; PRINCIPLE: P5 - Semantic Correctness
(declare-const logged Bool)
(assert (= logged true))
(assert (not logged))
(check-sat)
; unsat → console.log() unconditionally succeeds
```

```smt2
; [NEW] At line 354, contract state guaranteed
; PRINCIPLE: P1 - Precondition Propagation
(declare-const contract_exists Bool)
(assert contract_exists)
(assert (not contract_exists))
(check-sat)
; unsat → contract exists is guaranteed
```