fn longer<'a, 'b: 'a>(x: &'a u32, _y: &'b u32) -> &'a u32 {
    x
}
