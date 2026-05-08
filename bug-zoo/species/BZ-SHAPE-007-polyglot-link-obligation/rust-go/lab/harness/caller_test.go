package lab

import "testing"

func process(n int) int {
	return n * 2
}

func caller(n int) int {
	return process(n)
}

func TestHostCheckPassesWithoutCrossKitContract(t *testing.T) {
	if got := caller(-1); got != -2 {
		t.Fatalf("caller(-1) = %d, want -2", got)
	}
}
