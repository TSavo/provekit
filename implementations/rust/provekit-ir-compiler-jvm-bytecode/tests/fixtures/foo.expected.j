.class public Foo
.super java/lang/Object

.method public static foo(I)I
  .limit stack 2
  .limit locals 1
  iload 0
  iconst_0
  if_icmpeq L_true_0
  iconst_0
  goto L_end_1
L_true_0:
  iconst_1
L_end_1:
  ifeq L_else_2
  bipush 22
  ineg
  ireturn
L_else_2:
L_end_3:
  iload 0
  ireturn
.end method
