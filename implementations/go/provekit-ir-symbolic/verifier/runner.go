package verifier

// Runner is the orchestrator. It composes the 6 stages into a single
// RunBridgeEnforcement entry point + uses goroutines + a channel to
// process per-callsite verifications in parallel (each callsite's
// resolve→instantiate→solve chain is independent, so they fan out).
type Runner struct {
	loadStage        *LoadAllProofsStage
	enumerateStage   *EnumerateCallsitesStage
	resolveStage     *ResolveTargetStage
	instantiateStage *InstantiateStage
	solveStage       *SolveObligationStage
	reportStage      *ReportStage
	solver           *Solver
}

// NewRunner constructs a Runner with the configured solver.
func NewRunner(solver *Solver) *Runner {
	return &Runner{
		loadStage:        &LoadAllProofsStage{},
		enumerateStage:   &EnumerateCallsitesStage{},
		resolveStage:     &ResolveTargetStage{},
		instantiateStage: &InstantiateStage{},
		solveStage:       NewSolveObligationStage(),
		reportStage:      &ReportStage{},
		solver:           solver,
	}
}

// RunBridgeEnforcement executes the full pipeline:
//   1. load-all-proofs
//   2. enumerate-callsites
//   3+4+5. for each callsite (in parallel): resolve, instantiate, solve
//   6. report
func (r *Runner) RunBridgeEnforcement(projectRoot string) (*Report, error) {
	pool, err := r.loadStage.Run(projectRoot)
	if err != nil {
		return nil, err
	}
	callsites := r.enumerateStage.Run(pool)

	// Fan out per-callsite work via goroutines + a channel. Order is
	// preserved by indexing into a result slice.
	rows := make([]ReportRow, len(callsites))
	type completion struct {
		idx int
		row ReportRow
	}
	ch := make(chan completion, len(callsites))
	for i, cs := range callsites {
		go func(idx int, cs CallSite) {
			ch <- completion{idx: idx, row: r.processCallSite(cs, pool)}
		}(i, cs)
	}
	for i := 0; i < len(callsites); i++ {
		c := <-ch
		rows[c.idx] = c.row
	}
	return r.reportStage.Run(rows, pool.LoadErrors), nil
}

// processCallSite runs a single callsite's resolve→instantiate→solve
// chain. Returns one ReportRow.
func (r *Runner) processCallSite(cs CallSite, pool *MementoPool) ReportRow {
	resolved, fail := r.resolveStage.Run(cs, pool)
	if resolved == nil {
		return ReportRow{CallSite: cs, Status: "unresolved-target", Reason: fail}
	}
	if cs.ArgTerm == nil {
		return ReportRow{CallSite: cs, Status: "unliftable-argument", Reason: "no first arg"}
	}
	obligation, err := r.instantiateStage.Run(resolved.IRFormula, cs.ArgTerm)
	if err != nil {
		return ReportRow{CallSite: cs, Status: "lift-error", Reason: err.Error()}
	}
	solveResult, err := r.solveStage.Run(obligation, r.solver)
	if err != nil {
		return ReportRow{CallSite: cs, Status: "lift-error", Reason: err.Error()}
	}
	return ReportRow{CallSite: cs, Status: string(solveResult.Verdict)}
}
