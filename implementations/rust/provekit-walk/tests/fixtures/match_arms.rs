fn f(x: u32) {
    match x {
        0 | 1 => panic!(),
        _ => {}
    }
}
