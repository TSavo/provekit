package main

func checkPositive(x int) bool {
    if x <= 0 { return false }
    return true
}

func callerSatisfiesPre() bool {
    checkPositive(5)
}

func callerViolatesPre() bool {
    checkPositive(-1)
}
