package main

func sum_to(n int) int {
    s := 0
    i := 0
    for i < n {
        s = s + i
        i = i + 1
    }
    return s
}
