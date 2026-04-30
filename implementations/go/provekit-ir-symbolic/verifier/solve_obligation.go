package verifier

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
	"sync"
	"time"
)

// SolverEntry describes one solver binary (z3, cvc5, ...) the
// SolveObligationStage will invoke. Compiler choice is "smt-lib"
// for now; other compilers (lean, coq) are out of scope until the
// IR translators land.
type SolverEntry struct {
	Type     string
	Binary   string
	Flags    []string
	Compiler string
	Timeout  time.Duration
}

// Solver is the protocol's solver abstraction: one or more leaf
// entries. Multi-entry: all run in parallel via goroutines/channels;
// verdict is the consensus or "disagreement".
type Solver struct {
	Entries []SolverEntry
}

// SolveObligationStage runs SMT-LIB on the configured solver(s). It
// translates the IR obligation to SMT-LIB, invokes each SolverEntry
// in parallel via channels, and returns the unified verdict.
type SolveObligationStage struct {
	emitter *SMTEmitter
}

// NewSolveObligationStage constructs the stage with a fresh SMTEmitter.
func NewSolveObligationStage() *SolveObligationStage {
	return &SolveObligationStage{emitter: NewSMTEmitter()}
}

// EntryProbe is a single entry's verdict for forensic transparency.
type EntryProbe struct {
	SolverType string
	Probe      string  // "sat" / "unsat" / "unknown" / "timeout"
}

// SolveResult is the composed verdict + per-entry detail.
type SolveResult struct {
	Verdict ObligationVerdict
	PerEntry []EntryProbe
	Script  string  // SMT-LIB script (for debugging / audit)
}

// Run translates the obligation to SMT-LIB and invokes the solver(s)
// in parallel. The probe asks: is `(not OBLIGATION)` SAT?
//   - unsat → no counter-example → obligation holds → DISCHARGED
//   - sat   → counter-example exists → obligation fails → UNSATISFIED
//   - unknown / timeout → UNDECIDABLE
func (s *SolveObligationStage) Run(obligation interface{}, solver *Solver) (*SolveResult, error) {
	smt, err := s.emitter.EmitProbe(obligation)
	if err != nil {
		return nil, err
	}

	// Spawn one goroutine per solver entry; collect verdicts via a channel.
	type result struct {
		idx   int
		probe string
	}
	ch := make(chan result, len(solver.Entries))
	var wg sync.WaitGroup
	for i, entry := range solver.Entries {
		wg.Add(1)
		go func(idx int, e SolverEntry) {
			defer wg.Done()
			probe := invokeSolver(e, smt)
			ch <- result{idx: idx, probe: probe}
		}(i, entry)
	}
	wg.Wait()
	close(ch)

	probes := make([]EntryProbe, len(solver.Entries))
	for r := range ch {
		probes[r.idx] = EntryProbe{
			SolverType: solver.Entries[r.idx].Type,
			Probe:      r.probe,
		}
	}

	// Compose the final verdict.
	allAgree := true
	first := probes[0].Probe
	for _, p := range probes[1:] {
		if p.Probe != first {
			allAgree = false
			break
		}
	}
	var verdict ObligationVerdict
	switch {
	case !allAgree:
		verdict = VerdictDisagreement
	case first == "unsat":
		verdict = VerdictDischarged
	case first == "sat":
		verdict = VerdictUnsatisfied
	default:
		verdict = VerdictUndecidable
	}
	return &SolveResult{Verdict: verdict, PerEntry: probes, Script: smt}, nil
}

// invokeSolver runs one solver binary, feeding script on stdin.
// Returns "sat" / "unsat" / "unknown" / "timeout".
func invokeSolver(e SolverEntry, script string) string {
	timeout := e.Timeout
	if timeout == 0 {
		timeout = 5 * time.Second
	}
	args := make([]string, len(e.Flags))
	for i, f := range e.Flags {
		f = strings.ReplaceAll(f, "{{TIMEOUT_S}}", fmt.Sprintf("%d", int(timeout.Seconds())))
		f = strings.ReplaceAll(f, "{{TIMEOUT_MS}}", fmt.Sprintf("%d", int(timeout.Milliseconds())))
		args[i] = f
	}
	ctx, cancel := context.WithTimeout(context.Background(), timeout+250*time.Millisecond)
	defer cancel()
	cmd := exec.CommandContext(ctx, e.Binary, args...)
	cmd.Stdin = strings.NewReader(script)
	out, err := cmd.Output()
	if ctx.Err() == context.DeadlineExceeded {
		return "timeout"
	}
	if err != nil {
		return "unknown"
	}
	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	last := strings.TrimSpace(lines[len(lines)-1])
	switch last {
	case "sat", "unsat", "unknown":
		return last
	default:
		return "unknown"
	}
}
