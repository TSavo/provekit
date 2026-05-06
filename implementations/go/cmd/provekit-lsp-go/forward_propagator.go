package forward_propagator

import "fmt"

// Post represents an accumulated post-condition.
type Post struct {
	Constraints []string
	IsTop       bool
}

// Top returns a top (uninformative) post.
func Top() Post {
	return Post{Constraints: nil, IsTop: true}
}

// Of returns a post with a single constraint.
func Of(constraint string) Post {
	return Post{Constraints: []string{constraint}, IsTop: false}
}

// DiagnosticResult represents an implication failure.
type DiagnosticResult struct {
	Code    string
	Message string
}

// ForwardPropagator accumulates posts and checks callsites.
type ForwardPropagator struct {
	seedCatalog map[string]Post
}

// New creates a new ForwardPropagator.
func New() *ForwardPropagator {
	return &ForwardPropagator{
		seedCatalog: make(map[string]Post),
	}
}

// AddToCatalog adds a callee's post to the seed catalog.
func (fp *ForwardPropagator) AddToCatalog(calleeID string, pre Post, post Post) {
	fp.seedCatalog[calleeID] = post
}

// CheckCallsite checks if currentPost implies calleeID's pre.
func (fp *ForwardPropagator) CheckCallsite(calleeID string, currentPost Post) *DiagnosticResult {
	if currentPost.IsTop {
		return nil
	}
	calleePre, ok := fp.seedCatalog[calleeID]
	if !ok {
		return nil
	}
	for _, c := range currentPost.Constraints {
		found := false
		for _, cp := range calleePre.Constraints {
			if c == cp {
				found = true
				break
			}
		}
		if !found {
			return &DiagnosticResult{
				Code:    "implication-failed",
				Message: fmt.Sprintf("post does not imply callee pre: %v", calleePre.Constraints),
			}
		}
	}
	return nil
}
