package main

func classify(x int) int {
    y := 0
    if (x > 0 && x < 10) {
        y = 1
    } else {
        y = 2
    }
    return y
}
