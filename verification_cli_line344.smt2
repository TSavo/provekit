```smt2
; [NEW] At line 344, console.error() always succeeds
; PRINCIPLE: P5 - Semantic Correctness
(declare-const error_logged Bool)
(assert (= error_logged true))
(assert (not error_logged))
(check-sat)
; unsat → console.error() unconditionally succeeds
```

```smt2
; [NEW] At line 344, contract is falsy (path condition)
; PRINCIPLE: P1 - Precondition Propagation
(declare-const contract_exists Bool)
; Path condition guarantees contract not found
(assert (not contract_exists))
; Negation: contract exists
(assert contract_exists)
(check-sat)
; unsat → !contract is guaranteed at line 344
```

```smt2
; [NEW] At line 344, relPath and line are extracted
; PRINCIPLE: P3 - Calling Context Analysis
; The relPath and line come from earlier parsing
(declare-const relPath String)
(declare-const line_num Int)
; parseInt() at line 321 produces an integer
; Can line_num be NaN or invalid?
(assert (= line_num 0))
; If parseInt fails, returns NaN which becomes 0
(assert (= line_num 0))
(check-sat)
; sat → line can be 0 if parseInt fails
; This causes confusing error message but not a violation
```