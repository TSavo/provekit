; format: bridgeworks.toy8.asm.v1
; mutation_id: lowering_ignores_carry
; expected_refusal: missing compiler.lowering.preserves_checked_add

.function checked_add_u8
  ; inputs: r0 = a:u8, r1 = b:u8
  ; BUG: carry from ADD8 is ignored, so overflow returns ok.

  ADD8 r2, r0, r1
  RET_OK r2
.end
