; format: bridgeworks.toy8.asm.v1
; claim_id: compiler.lowering.preserves_checked_add
; preserves: checked_add_u8 via ADD8 plus branch-on-carry

.function checked_add_u8
  ; inputs: r0 = a:u8, r1 = b:u8
  ; output ok: overflow=false, value=r2
  ; output overflow: overflow=true, value=0

  ADD8 r2, r0, r1
  BR_CARRY overflow
  RET_OK r2

overflow:
  RET_OVERFLOW
.end
