package sample

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// TestId is the harvested callsite for the declared boundary. The leaf
// harvester lifts `assert.Equal(t, Id(3), 3)` to a contract whose
// `inv = =(Id(3), 3)`, which the verifier reduces through the body-derived
// contract for `Id` (`post = result == x`) -> `3 == 3` -> z3 discharges.
func TestId(t *testing.T) {
	assert.Equal(t, Id(3), 3)
}
