pub fn sum_to(n: i32) -> i32 {
    let mut s = 0;
    let mut i = 0;
    while i < n {
        s = s + i;
        i = i + 1;
    }
    return s;
}
