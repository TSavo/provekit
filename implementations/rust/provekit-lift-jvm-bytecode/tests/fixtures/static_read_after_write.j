.class public StaticReadAfterWrite
.super java/lang/Object

.field public static value I

.method public static stale()I
  .limit stack 1
  .limit locals 2
  getstatic StaticReadAfterWrite/value I
  istore_0
  iconst_5
  putstatic StaticReadAfterWrite/value I
  getstatic StaticReadAfterWrite/value I
  istore_1
  iload_1
  ireturn
.end method
