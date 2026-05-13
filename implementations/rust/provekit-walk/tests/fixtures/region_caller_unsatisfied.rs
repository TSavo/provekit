fn callee<'a, 'b: 'a>(x: &'a u32, y: &'b u32) -> &'a u32 {
    let _ = y;
    x
}

fn caller<'a, 'b>(x: &'a u32, y: &'b u32) -> &'a u32 {
    callee(x, y)
}
