# #115 step 2: manual-30 gate

Mechanical-tagger-v1 says these 30 candidates have these tags. For each row,
open the candidate's diff (`cd /Users/tsavo/bugsjs/<project> && git diff Bug-<id>..Bug-<id>-fix`)
and tick exactly ONE of: agree / disagree / unclear.

**Tag legend**
- expressible-now-recognized → an existing principle in our library matches the bug locus
- expressible-now-pending-principle → substrate covers signature; no principle yet
- needs-new-relation → multi-node relation absent (chain, alias, composition)
- unknown → tagger could not classify mechanically

**Disagreement counts as miss-tag for the precision number.** The 90% gate
(27 agree / 30 total = 90%) is necessary to proceed to step 3.

Sample salt: `115-step2-v1` (re-run `scripts/sample-30.ts` reproduces this exact list).

## expressible-now-pending-principle (16 sampled / 252 total)

### 1. pencilblue/Bug-6

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 9 capabilities cover dirty nodes [assigns,returns,member_access,truthiness,narrows,decides,calls,captures,binding]; cross/intra=68/42 (62%); no principle fires yet

**Signature columns:** ["assigns.assign_kind","assigns.node_id","assigns.rhs_node","assigns.target_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","captures.captured_name","captures.mutable","captures.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["assigns.assign_kind:=","binding.binding_kind:function","binding.binding_kind:param","binding.binding_kind:var","decides.decision_kind:if","decides.decision_kind:short_circuit_and","decides.decision_kind:short_circuit_or","decides.decision_kind:ternary","narrows.narrowing_kind:literal_eq","narrows.narrowing_kind:null_check","narrows.narrowing_kind:undefined_check","returns.exit_kind:return","truthiness.coercion_kind:falsy_default","truthiness.coercion_kind:strict_eq_null","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/pencilblue && git diff Bug-6..Bug-6-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 2. eslint/Bug-72

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,narrows,decides,calls,binding]; cross/intra=75/6 (93%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node"]
**Signature kinds:** ["binding.binding_kind:param","decides.decision_kind:if","narrows.narrowing_kind:literal_eq","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-72..Bug-72-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 3. eslint/Bug-259

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 7 capabilities cover dirty nodes [returns,member_access,truthiness,narrows,decides,calls,binding]; cross/intra=37/46 (45%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","decides.decision_kind:short_circuit_and","decides.decision_kind:short_circuit_or","decides.decision_kind:ternary","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:falsy_default","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-259..Bug-259-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 4. eslint/Bug-78

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 5 capabilities cover dirty nodes [member_access,truthiness,decides,calls,binding]; cross/intra=59/12 (83%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-78..Bug-78-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 5. eslint/Bug-291

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,decides,calls,captures,binding]; cross/intra=18/3 (86%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","captures.captured_name","captures.mutable","captures.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node"]
**Signature kinds:** ["binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:short_circuit_and","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-291..Bug-291-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 6. eslint/Bug-262

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [member_access,truthiness,narrows,decides,calls,binding]; cross/intra=136/54 (72%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","narrows.narrowing_kind:literal_eq","narrows.narrowing_kind:null_check","truthiness.coercion_kind:strict_eq_null"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-262..Bug-262-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 7. eslint/Bug-127

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,truthiness,decides,calls,binding]; cross/intra=127/14 (90%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","decides.decision_kind:short_circuit_or","returns.exit_kind:return","truthiness.coercion_kind:falsy_default","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-127..Bug-127-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 8. eslint/Bug-208

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,narrows,decides,calls,binding]; cross/intra=32/7 (82%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:if","narrows.narrowing_kind:literal_eq","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-208..Bug-208-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 9. eslint/Bug-327

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 7 capabilities cover dirty nodes [returns,member_access,truthiness,narrows,decides,calls,binding]; cross/intra=14/7 (67%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:if","decides.decision_kind:short_circuit_and","decides.decision_kind:short_circuit_or","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:falsy_default"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-327..Bug-327-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 10. eslint/Bug-80

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 8 capabilities cover dirty nodes [assigns,returns,member_access,truthiness,narrows,decides,calls,binding]; cross/intra=196/37 (84%); no principle fires yet

**Signature columns:** ["assigns.assign_kind","assigns.node_id","assigns.rhs_node","assigns.target_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["assigns.assign_kind:=","binding.binding_kind:param","decides.decision_kind:if","narrows.narrowing_kind:in","returns.exit_kind:return","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-80..Bug-80-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 11. eslint/Bug-82

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 7 capabilities cover dirty nodes [returns,member_access,truthiness,narrows,decides,calls,binding]; cross/intra=68/21 (76%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","decides.decision_kind:short_circuit_and","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-82..Bug-82-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 12. eslint/Bug-96

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 8 capabilities cover dirty nodes [assigns,returns,member_access,truthiness,narrows,decides,calls,binding]; cross/intra=97/3 (97%); no principle fires yet

**Signature columns:** ["assigns.assign_kind","assigns.node_id","assigns.rhs_node","assigns.target_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["assigns.assign_kind:=","binding.binding_kind:const","binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:short_circuit_or","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:falsy_default"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-96..Bug-96-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 13. eslint/Bug-196

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 7 capabilities cover dirty nodes [assigns,returns,member_access,narrows,decides,calls,binding]; cross/intra=208/28 (88%); no principle fires yet

**Signature columns:** ["assigns.assign_kind","assigns.node_id","assigns.rhs_node","assigns.target_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node"]
**Signature kinds:** ["assigns.assign_kind:+=","assigns.assign_kind:=","binding.binding_kind:function","binding.binding_kind:param","binding.binding_kind:var","decides.decision_kind:if","decides.decision_kind:short_circuit_and","narrows.narrowing_kind:literal_eq","narrows.narrowing_kind:null_check","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-196..Bug-196-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 14. eslint/Bug-23

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 5 capabilities cover dirty nodes [returns,member_access,calls,captures,binding]; cross/intra=4/9 (31%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","captures.captured_name","captures.mutable","captures.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:function","binding.binding_kind:param","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-23..Bug-23-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 15. eslint/Bug-24

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,decides,calls,binding,signal_interpolations]; cross/intra=25/9 (74%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node","signal_interpolations.interpolated_node","signal_interpolations.signal_node","signal_interpolations.slot_index"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-24..Bug-24-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 16. eslint/Bug-11

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 7 capabilities cover dirty nodes [assigns,returns,member_access,truthiness,narrows,decides,calls]; cross/intra=20/0 (100%); no principle fires yet

**Signature columns:** ["assigns.assign_kind","assigns.node_id","assigns.rhs_node","assigns.target_node","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["assigns.assign_kind:=","decides.decision_kind:if","decides.decision_kind:short_circuit_and","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-11..Bug-11-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

## expressible-now-recognized (11 sampled / 147 total)

### 17. karma/Bug-22

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[addition-overflow] hit at locus

**Matched principles:** ["addition-overflow"]

Diff: `cd /Users/tsavo/bugsjs/karma && git diff Bug-22..Bug-22-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 18. eslint/Bug-51

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[addition-overflow] hit at locus

**Matched principles:** ["addition-overflow"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-51..Bug-51-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 19. express/Bug-21

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[falsy-default] hit at locus

**Matched principles:** ["falsy-default"]

Diff: `cd /Users/tsavo/bugsjs/express && git diff Bug-21..Bug-21-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 20. eslint/Bug-232

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[addition-overflow,multiplication-overflow] hit at locus

**Matched principles:** ["addition-overflow","multiplication-overflow"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-232..Bug-232-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 21. eslint/Bug-246

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[addition-overflow,subtraction-underflow] hit at locus

**Matched principles:** ["addition-overflow","subtraction-underflow"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-246..Bug-246-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 22. eslint/Bug-60

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[multiplication-overflow] hit at locus

**Matched principles:** ["multiplication-overflow"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-60..Bug-60-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 23. eslint/Bug-64

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[addition-overflow] hit at locus

**Matched principles:** ["addition-overflow"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-64..Bug-64-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 24. eslint/Bug-182

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[subtraction-underflow] hit at locus

**Matched principles:** ["subtraction-underflow"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-182..Bug-182-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 25. hexo/Bug-12

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[addition-overflow] hit at locus

**Matched principles:** ["addition-overflow"]

Diff: `cd /Users/tsavo/bugsjs/hexo && git diff Bug-12..Bug-12-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 26. eslint/Bug-49

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[addition-overflow] hit at locus

**Matched principles:** ["addition-overflow"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-49..Bug-49-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 27. hessian.js/Bug-6

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[subtraction-underflow] hit at locus

**Matched principles:** ["subtraction-underflow"]

Diff: `cd /Users/tsavo/bugsjs/hessian.js && git diff Bug-6..Bug-6-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

## needs-new-relation (1 sampled / 1 total)

### 28. eslint/Bug-244

**Tagger says:** `needs-new-relation`

**Audit line:** needs-new-relation: 1 capability + 26/26 cross-locus edges (100%); capabilities=[binding]

**Missing:** cols=[] relations=["data_flow_chain (transitive across locus boundary)"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-244..Bug-244-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

## unknown (2 sampled / 3 total)

### 29. eslint/Bug-44

**Tagger says:** `unknown`

**Audit line:** unknown: 1 capability(ies) on 35 dirty nodes (below threshold 2); kinds=[ConstKeyword,EqualsToken,Identifier,JSDoc,RegularExpressionLiteral,SemicolonToken,SingleLineCommentTrivia,SyntaxList,VariableDeclaration,VariableDeclarationList,VariableStatement]


Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-44..Bug-44-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---

### 30. pencilblue/Bug-7

**Tagger says:** `unknown`

**Audit line:** unknown: no files indexed (parser failed on every changed file)


Diff: `cd /Users/tsavo/bugsjs/pencilblue && git diff Bug-7..Bug-7-fix`

- [ ] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:**

---
