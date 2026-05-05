fn inner(y: u32) -> u32 {
    if y < 5 {
        panic!();
    }
    y
}

fn outer(x: u32) -> u32 {
    inner(x)
}
