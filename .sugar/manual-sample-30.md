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

## expressible-now-pending-principle (16 sampled / 318 total)

### 1. karma/Bug-22

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 4 capabilities cover dirty nodes [arithmetic,returns,member_access,binding]; cross/intra=100/1 (99%); no principle fires yet

**Signature columns:** ["arithmetic.lhs_node","arithmetic.node_id","arithmetic.op","arithmetic.result_sort","arithmetic.rhs_node","binding.binding_kind","binding.name","binding.node_id","member_access.computed","member_access.node_id","member_access.object_node","returns.exit_kind","returns.node_id","returns.value_node"]
**Signature kinds:** ["arithmetic.op:+","arithmetic.result_sort:String","binding.binding_kind:var","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/karma && git diff Bug-22..Bug-22-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix concatenates `config.hostname` and `config.port` into URL_REGEXP source string; arithmetic.op:+ on String result_sort plus var binding plausibly covers the shape, even though load-bearing change is regex content.

---

### 2. eslint/Bug-51

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 10 capabilities cover dirty nodes [arithmetic,assigns,returns,member_access,truthiness,decides,iterates,calls,captures,binding]; cross/intra=6/20 (23%); no principle fires yet

**Signature columns:** ["arithmetic.lhs_node","arithmetic.node_id","arithmetic.op","arithmetic.result_sort","arithmetic.rhs_node","assigns.assign_kind","assigns.node_id","assigns.rhs_node","assigns.target_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","captures.captured_name","captures.mutable","captures.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","iterates.body_node","iterates.condition_node","iterates.executes_at_least_once","iterates.loop_kind","iterates.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["arithmetic.op:+","arithmetic.result_sort:String","assigns.assign_kind:=","binding.binding_kind:function","binding.binding_kind:param","binding.binding_kind:var","decides.decision_kind:if","decides.decision_kind:short_circuit_and","iterates.loop_kind:while","returns.exit_kind:return","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-51..Bug-51-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix adds new `label` parameter, branches on `node.type` to pick a sentinel regex, tracks `labelInside`. decides.if/short_circuit_and, while-loop, truthy_test all match the bug shape; no principle yet covers the labeled-break-in-finally semantics.

---

### 3. express/Bug-21

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 9 capabilities cover dirty nodes [assigns,returns,member_access,truthiness,narrows,decides,calls,captures,binding]; cross/intra=219/115 (66%); no principle fires yet

**Signature columns:** ["assigns.assign_kind","assigns.node_id","assigns.rhs_node","assigns.target_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","captures.captured_name","captures.mutable","captures.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["assigns.assign_kind:=","binding.binding_kind:function","binding.binding_kind:param","binding.binding_kind:var","decides.decision_kind:if","decides.decision_kind:short_circuit_or","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:falsy_default"]

Diff: `cd /Users/tsavo/bugsjs/express && git diff Bug-21..Bug-21-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Major refactor replacing `wrap` closure with `restore` helper to save/restore multiple req props; assigns/captures/calls/decides cover the shape, no single principle captures the closure-state-restoration pattern.

---

### 4. pencilblue/Bug-6

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 9 capabilities cover dirty nodes [assigns,returns,member_access,truthiness,narrows,decides,calls,captures,binding]; cross/intra=68/42 (62%); no principle fires yet

**Signature columns:** ["assigns.assign_kind","assigns.node_id","assigns.rhs_node","assigns.target_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","captures.captured_name","captures.mutable","captures.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["assigns.assign_kind:=","binding.binding_kind:function","binding.binding_kind:param","binding.binding_kind:var","decides.decision_kind:if","decides.decision_kind:short_circuit_and","decides.decision_kind:short_circuit_or","decides.decision_kind:ternary","narrows.narrowing_kind:literal_eq","narrows.narrowing_kind:null_check","narrows.narrowing_kind:undefined_check","returns.exit_kind:return","truthiness.coercion_kind:falsy_default","truthiness.coercion_kind:strict_eq_null","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/pencilblue && git diff Bug-6..Bug-6-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix swaps validation predicate (isFloat→isNum+>0), tightens email regex, adds array empty-check via `_.isEqual(val, [])`. decides/narrows/truthiness/calls cover; no principle yet for "validation predicate accepts wrong domain".

---

### 5. eslint/Bug-72

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,narrows,decides,calls,binding]; cross/intra=75/6 (93%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node"]
**Signature kinds:** ["binding.binding_kind:param","decides.decision_kind:if","narrows.narrowing_kind:literal_eq","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-72..Bug-72-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix swaps `lastItem.loc.end` to `penultimateToken.loc.end` for report location. Member-access columns + binding param + decides.if are present; no principle yet captures "wrong source location for error report".

---

### 6. eslint/Bug-259

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 7 capabilities cover dirty nodes [returns,member_access,truthiness,narrows,decides,calls,binding]; cross/intra=37/46 (45%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","decides.decision_kind:short_circuit_and","decides.decision_kind:short_circuit_or","decides.decision_kind:ternary","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:falsy_default","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-259..Bug-259-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix adds `lastToken &&` guard before passing it to `astUtils.isClosingBraceToken(lastToken)`. truthiness.short_circuit_and + truthy_test + ternary cover the null-guard shape; no `null-call-recipient-guard` principle yet.

---

### 7. eslint/Bug-78

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 5 capabilities cover dirty nodes [member_access,truthiness,decides,calls,binding]; cross/intra=59/12 (83%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-78..Bug-78-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** VariableDeclarator indent fix introduces `tokenAfterOperator` const, calls `offsets.matchIndentOf`. const binding + decides.if + calls + member_access plausibly cover; no specific principle exists.

---

### 8. eslint/Bug-60

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 8 capabilities cover dirty nodes [arithmetic,returns,member_access,truthiness,narrows,decides,calls,binding]; cross/intra=155/5 (97%); no principle fires yet

**Signature columns:** ["arithmetic.lhs_node","arithmetic.node_id","arithmetic.op","arithmetic.result_sort","arithmetic.rhs_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["arithmetic.op:*","arithmetic.result_sort:Numeric","binding.binding_kind:param","decides.decision_kind:if","decides.decision_kind:short_circuit_and","narrows.narrowing_kind:literal_eq","narrows.narrowing_kind:null_check","returns.exit_kind:return","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-60..Bug-60-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix changes `indentSize * options.X.parameters` to `getNodeIndent(node).goodChar + indentSize * options.X.parameters`. arithmetic.op:* numeric + member_access + calls plausibly cover; "missing base offset" not encoded as a principle yet.

---

### 9. eslint/Bug-291

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,decides,calls,captures,binding]; cross/intra=18/3 (86%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","captures.captured_name","captures.mutable","captures.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node"]
**Signature kinds:** ["binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:short_circuit_and","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-291..Bug-291-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix swaps `name[0] === toLocaleUpperCase()` to `name[0] !== toLocaleLowerCase()` to handle non-cased characters. member_access + calls + short_circuit_and cover the comparison shape; no principle for "case-class predicate must use negative form".

---

### 10. eslint/Bug-262

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [member_access,truthiness,narrows,decides,calls,binding]; cross/intra=136/54 (72%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","narrows.narrowing_kind:literal_eq","narrows.narrowing_kind:null_check","truthiness.coercion_kind:strict_eq_null"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-262..Bug-262-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix splits ExportNamedDeclaration handling so `from 'qux'` tokens get proper offset; adds new ImportDeclaration `from` block. member_access/calls/decides cover; no principle yet for "missed token range needs offset".

---

### 11. eslint/Bug-127

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,truthiness,decides,calls,binding]; cross/intra=127/14 (90%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","decides.decision_kind:short_circuit_or","returns.exit_kind:return","truthiness.coercion_kind:falsy_default","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-127..Bug-127-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix replaces `getLastToken(node)` with `getTokenAfter(lodash.findLast(node.elements) || openingBracket, isClosingBracketToken)` for arrays/objects. falsy_default + calls + member_access cover; no principle for "wrong token boundary picker".

---

### 12. eslint/Bug-64

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 7 capabilities cover dirty nodes [arithmetic,returns,member_access,truthiness,decides,calls,binding]; cross/intra=149/19 (89%); no principle fires yet

**Signature columns:** ["arithmetic.lhs_node","arithmetic.node_id","arithmetic.op","arithmetic.result_sort","arithmetic.rhs_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["arithmetic.op:+","arithmetic.result_sort:Unknown","binding.binding_kind:const","binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:if","decides.decision_kind:short_circuit_and","decides.decision_kind:ternary","returns.exit_kind:return","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-64..Bug-64-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix introduces `requiresTrailingSpace` and adds left-side reports for ForIn/ForOf statements. arithmetic + decides + calls + member_access cover; no principle for "asymmetric AST traversal missing left side".

---

### 13. eslint/Bug-208

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 6 capabilities cover dirty nodes [returns,member_access,narrows,decides,calls,binding]; cross/intra=32/7 (82%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id"]
**Signature kinds:** ["binding.binding_kind:const","binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:if","narrows.narrowing_kind:literal_eq","returns.exit_kind:return"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-208..Bug-208-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix adds DIVISION_MESSAGE constant + new visitor handler for `BinaryExpression[operator='/'] > BinaryExpression[operator='/']`. binding/calls/decides/narrows cover; no principle yet for "missing detector branch".

---

### 14. eslint/Bug-182

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 8 capabilities cover dirty nodes [arithmetic,returns,member_access,truthiness,decides,calls,binding,signal_interpolations]; cross/intra=236/20 (92%); no principle fires yet

**Signature columns:** ["arithmetic.lhs_node","arithmetic.node_id","arithmetic.op","arithmetic.result_sort","arithmetic.rhs_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","returns.exit_kind","returns.node_id","returns.value_node","signal_interpolations.interpolated_node","signal_interpolations.signal_node","signal_interpolations.slot_index","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["arithmetic.op:-","arithmetic.result_sort:Numeric","binding.binding_kind:const","binding.binding_kind:param","decides.decision_kind:if","returns.exit_kind:return","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-182..Bug-182-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix introduces a guard: if any commentLine starts with `/`, skip the autofix (return null). decides.if + truthy_test + calls + member_access cover; no principle yet for "autofix must abort on ambiguous source".

---

### 15. eslint/Bug-327

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 7 capabilities cover dirty nodes [returns,member_access,truthiness,narrows,decides,calls,binding]; cross/intra=14/7 (67%); no principle fires yet

**Signature columns:** ["binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:if","decides.decision_kind:short_circuit_and","decides.decision_kind:short_circuit_or","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:falsy_default"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-327..Bug-327-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix adds `node.argument && containsAssignment(node.argument)` null-guard before recursing on optional ReturnStatement argument. truthy_test/short_circuit_and/decides cover the null-arg-recursion guard shape.

---

### 16. eslint/Bug-49

**Tagger says:** `expressible-now-pending-principle`

**Audit line:** expressible-now-pending-principle: 10 capabilities cover dirty nodes [arithmetic,returns,member_access,truthiness,narrows,decides,calls,captures,binding,signal_interpolations]; cross/intra=181/189 (49%); no principle fires yet

**Signature columns:** ["arithmetic.lhs_node","arithmetic.node_id","arithmetic.op","arithmetic.result_sort","arithmetic.rhs_node","binding.binding_kind","binding.name","binding.node_id","calls.arg_count","calls.callee_is_async","calls.callee_name","calls.callee_node","calls.is_method_call","calls.node_id","captures.captured_name","captures.mutable","captures.node_id","decides.alternate_node","decides.condition_node","decides.consequent_node","decides.decision_kind","decides.node_id","member_access.computed","member_access.node_id","member_access.object_node","member_access.property_name","narrows.narrowed_type","narrows.narrowing_kind","narrows.node_id","narrows.target_node","returns.exit_kind","returns.node_id","returns.value_node","signal_interpolations.interpolated_node","signal_interpolations.signal_node","signal_interpolations.slot_index","truthiness.coercion_kind","truthiness.node_id","truthiness.operand_node"]
**Signature kinds:** ["arithmetic.op:+","arithmetic.result_sort:Unknown","binding.binding_kind:const","binding.binding_kind:function","binding.binding_kind:param","decides.decision_kind:if","decides.decision_kind:short_circuit_and","decides.decision_kind:ternary","narrows.narrowing_kind:literal_eq","returns.exit_kind:return","truthiness.coercion_kind:truthy_test"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-49..Bug-49-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix expands `replaceTextRange` end to `node.range[1]` and appends previously-dropped text via `sourceCode.text.slice(...)`. arithmetic + member_access + calls + decides cover; no principle yet for "fixer dropping trailing source".

---

## expressible-now-recognized (11 sampled / 81 total)

### 17. eslint/Bug-232

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[variable-staleness] hit at locus

**Matched principles:** ["variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-232..Bug-232-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix is a null-guard `parentElements[0] && ...` on member access, NOT a fall-through staleness pattern. variable-staleness's claim ("if-block writes a var also used outside") is irrelevant here. Should be expressible-now-pending-principle.

---

### 18. eslint/Bug-246

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[subtraction-underflow,variable-staleness] hit at locus

**Matched principles:** ["subtraction-underflow","variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-246..Bug-246-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix is a comment-padding logic rewrite using `getCommentsInNode` + `isLocatedBefore`. No subtraction whose LHS could underflow (the `commentGroup.length - 1` style is incidental); subtraction-underflow misfires. variable-staleness also doesn't fit: there's no fall-through with a default. Should be expressible-now-pending-principle.

---

### 19. hexo/Bug-12

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[or-chain-extended-by-fix] hit at locus

**Matched principles:** ["or-chain-extended-by-fix"]

Diff: `cd /Users/tsavo/bugsjs/hexo && git diff Bug-12..Bug-12-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix wraps `(data.slug || data.title).toString()`: the or-chain itself is unchanged; the post added a `.toString()` call, NOT a new clause. or-chain-extended-by-fix's semantic ("missing a clause") doesn't apply. The relation `was_replaced_by_addition` over-fires on any enclosing added node. Should be expressible-now-pending-principle.

---

### 20. eslint/Bug-80

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[variable-staleness] hit at locus

**Matched principles:** ["variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-80..Bug-80-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix is pure regex content tightening `/\{\{\s*(.+?)\s*\}\}/g` → `/\{\{\s*([^{}]+?)\s*\}\}/g`. No if-block, no assignment, no fall-through: variable-staleness has no business firing here. Should be unknown.

---

### 21. express/Bug-23

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[variable-staleness] hit at locus

**Matched principles:** ["variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/express && git diff Bug-23..Bug-23-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix replaces `if (typeof fn !== 'function') { offset = 1; path = fn; }` with a deeper check that walks arrays. The actual bug is "single-element array of fn passed to app.use mis-treated as path": this is a missing-case in narrowing, not a fall-through staleness. variable-staleness mechanically matches on `offset/path` but doesn't capture the bug's intent. Should be expressible-now-pending-principle.

---

### 22. eslint/Bug-196

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[variable-staleness] hit at locus

**Matched principles:** ["variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-196..Bug-196-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Two changes: `isOuterIIFE(calleeNode.parent)` → `isOuterIIFE(calleeNode)` (wrong-arg) plus `options.outerIIFEBody` → `options.outerIIFEBody * indentSize` (missing multiplication). The shape `var fo=indentSize; if (cond) {fo=X} use(fo)` exists but the bug is in the THEN branch's value, not fall-through. variable-staleness's claim doesn't fit. Should be expressible-now-pending-principle.

---

### 23. eslint/Bug-11

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[variable-staleness] hit at locus

**Matched principles:** ["variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-11..Bug-11-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix wraps `node.range[0] - (beforeCount || 0)` in `Math.max(..., 0)`: a clamp to prevent negative slice index (underflow). No if-block-with-assignment fall-through; variable-staleness is wrong. The right-shape principle would be subtraction-underflow, but the LHS isn't filtered through dirty-set in a way that catches it. Should be expressible-now-pending-principle.

---

### 24. eslint/Bug-184

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[or-chain-extended-by-fix] hit at locus

**Matched principles:** ["or-chain-extended-by-fix"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-184..Bug-184-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix is `expected = false` → `expected = leadingComments.length > 0`. No or-chain extension at all: the matched falsy_default node must be elsewhere in the file's surrounding code. Principle's claim "OR-chain was extended in fix" doesn't match this bug. Should be expressible-now-pending-principle.

---

### 25. eslint/Bug-323

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[variable-staleness] hit at locus

**Matched principles:** ["variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-323..Bug-323-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix removes `&& node.type === "FunctionDeclaration"` from else-if: broadens parameter-unused check to all parameter types, not just FunctionDeclaration. This is missing-case widening, not fall-through staleness. variable-staleness's claim doesn't fit. Should be expressible-now-pending-principle.

---

### 26. eslint/Bug-298

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[variable-staleness] hit at locus

**Matched principles:** ["variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-298..Bug-298-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix removes the `var right = null; if(left.type==="Identifier"){right=left;...}else{right=getFirstToken}` block entirely, replacing with early returns for class methods/shorthand and a single `var right = context.getFirstToken(node)`. The bug was wrong handling for class methods, not fall-through staleness. variable-staleness's claim doesn't fit. Should be expressible-now-pending-principle.

---

### 27. eslint/Bug-301

**Tagger says:** `expressible-now-recognized`

**Audit line:** expressible-now-recognized: principles=[multiplication-overflow,variable-staleness] hit at locus

**Matched principles:** ["multiplication-overflow","variable-staleness"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-301..Bug-301-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix discriminates `typeof options[parent.type] === "number"` vs string `"first"`. The multiplication is small indentSize-scale arithmetic: not overflow-prone: multiplication-overflow misfires (no overflow risk). variable-staleness mechanically matches `nodeIndent += ...` writes inside if, but the bug is missing string-option handling, not fall-through. Both principles wrong. Should be expressible-now-pending-principle.

---

## needs-new-relation (1 sampled / 1 total)

### 28. eslint/Bug-244

**Tagger says:** `needs-new-relation`

**Audit line:** needs-new-relation: 1 capability + 26/26 cross-locus edges (100%); capabilities=[binding]

**Missing:** cols=[] relations=["data_flow_chain (transitive across locus boundary)"]

Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-244..Bug-244-fix`

- [ ] agree (tagger correctly classified)
- [x] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix is regex content tightening: `/set(?:Timeout|Interval)|execScript/` → `/^(setTimeout|setInterval|execScript)$/`. The original alternation lacked anchors, matching substrings. Pure regex-literal semantics, not multi-node reasoning. Should be unknown.

---

## unknown (2 sampled / 3 total)

### 29. eslint/Bug-44

**Tagger says:** `unknown`

**Audit line:** unknown: 1 capability(ies) on 35 dirty nodes (below threshold 2); kinds=[ConstKeyword,EqualsToken,Identifier,JSDoc,RegularExpressionLiteral,SemicolonToken,SingleLineCommentTrivia,SyntaxList,VariableDeclaration,VariableDeclarationList,VariableStatement]


Diff: `cd /Users/tsavo/bugsjs/eslint && git diff Bug-44..Bug-44-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Fix is pure regex content: `/^(?:.+?Pattern|RestElement|Property)$/` → adds `SpreadProperty|ExperimentalRestProperty`. No structural code, just regex-literal alternation. Correctly classified as unknown (below capability threshold).

---

### 30. pencilblue/Bug-7

**Tagger says:** `unknown`

**Audit line:** unknown: no files indexed (parser failed on every changed file)


Diff: `cd /Users/tsavo/bugsjs/pencilblue && git diff Bug-7..Bug-7-fix`

- [x] agree (tagger correctly classified)
- [ ] disagree (provide correct tag in note)
- [ ] unclear (mark with ?)

**Note:** Multi-file change including param rename `protoype` → `prototype` plus added getBodyParsers function. Parser failed per audit; correctly classified as unknown.

---
