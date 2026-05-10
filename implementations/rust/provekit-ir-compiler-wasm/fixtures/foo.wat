(module
  (func $foo (export "foo") (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.eq
    if
      i32.const 0
      i32.const 22
      i32.sub
      return
    else
    end
    local.get $x
    return
  )
)
