package sample

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// TestDouble is the harvested callsite. The Go Layer-0 leaf harvester lifts
// `assert.Equal(t, Double(3), 6)` to a contract whose
// `inv = =(Double(3), 6)` -- the `=(<call>, <expected>)` shape the verifier's
// body-discharge seam enumerates and reduces through the body of `Double`.
func TestDouble(t *testing.T) {
	assert.Equal(t, Double(3), 6)
}
