.intel_syntax noprefix
.text
.globl foo
.type foo, @function
foo:
    test    edi, edi
    jne     .Lret
    mov     eax, -22
    ret
.Lret:
    mov     eax, edi
    ret
.size foo, .-foo
