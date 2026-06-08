fn f(x: u32) -> u32 {
    let offset = 10;
    let g = |y: u32| y + offset;
    g(x)
}
