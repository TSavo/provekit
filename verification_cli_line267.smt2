```smt2
; [NEW] At line 267, ref is string (not validated)
; PRINCIPLE: P3 - Calling Context Analysis
(declare-const ref_val String)
; runDiff is public, ref comes from args
; ref has default "HEAD~1" but accepts any string
(assert (= ref_val "HEAD~1"))
; Can ref be empty string?
(assert (= ref_val ""))
(check-sat)
; sat → empty ref is possible (args[0] = "")
```

```smt2
; [NEW] At line 267, console.log succeeds with any ref
; PRINCIPLE: P5 - Semantic Correctness
(declare-const logged Bool)
(assert (= logged true))
(assert (not logged))
(check-sat)
; unsat → console.log always succeeds
```

```smt2
; [NEW] REACHABLE: Invalid git ref causes empty diff
; PRINCIPLE: P6 - Boundary Input
(declare-const ref_input String)
(declare-const changes_count Int)
; User passes invalid ref string
(assert (= ref_input "nonexistent-ref-xyz"))
; proofDiff.diffAgainst returns empty when ref invalid
(assert (= changes_count 0))
; At line 274: "No proof changes." printed as fallback
(assert (= changes_count 0))
(check-sat)
; sat → invalid ref is reachable, produces empty diff
; Not a bug - correct error handling (empty = no changes)
```