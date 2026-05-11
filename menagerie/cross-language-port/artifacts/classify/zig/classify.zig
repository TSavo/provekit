pub fn classify(x: i32) i32 {
    var y: i32 = 0;
    if (x > 0 and x < 10) {
        y = 1;
    } else {
        y = 2;
    }
    return y;
}
