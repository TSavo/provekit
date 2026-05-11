pub fn sum_to(n: i32) i32 {
    var s: i32 = 0;
    var i: i32 = 0;
    while i < n {
        s = s + i;
        i = i + 1;
    }
    return s;
}
