.intel_syntax noprefix
.text
.globl foo
.type foo, @function
foo:
    mov     eax, edi
    cmp     eax, 0
    sete    al
    movzx   eax, al
    cmp     eax, 0
    je      .L_else_0
    mov     eax, 22
    neg     eax
    ret
.L_else_0:
.L_end_1:
    mov     eax, edi
    ret
.size foo, .-foo
.section .note.GNU-stack,"",@progbits
