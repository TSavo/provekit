.text
.globl foo
foo:
    cbz     w0, .Lerr
    ret
.Lerr:
    mov     w0, #-22
    ret
