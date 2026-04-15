; PRINCIPLE: P3 - Calling Context Analysis
; LINE: 94
; Bug: Regex at line 104 requires exact match ^\(assert\s+\(=\s+(\S+)\s+(\S+)\)\)
; Whitespace variation in assert causes regex to fail, silent false negative

(declare-const num_asserts Int)
; SMT2 has 2 asserts: (= x y) and (not (= x y)) but with extra spaces
; Code at line 98: asserts.length < 2 → false
(assert (= num_asserts 2))
; At line 104: regex /^\(assert\s+\(=\s+(\S+)\s+(\S+)\)\)$/ fails on "  "
; At line 105: does NOT add to equalities array
; At line 106-107: negation regex also fails
; At line 110-114: equalities array empty, returns false
; But this IS a trivial identity proof
(assert (not (= num_asserts 0)))
(check-sat)
; sat → returns false for actual trivial identity (whitespace breaks detection)