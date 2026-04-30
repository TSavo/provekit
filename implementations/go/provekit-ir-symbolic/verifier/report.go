package verifier

import "fmt"

// ReportStage aggregates per-callsite ReportRow records into a Report
// (totals + the rows). Final stage of the bridge enforcement workflow.
type ReportStage struct{}

// Run aggregates the rows + load errors into a Report.
func (s *ReportStage) Run(rows []ReportRow, loadErrors []LoadError) *Report {
	report := &Report{
		TotalCallsites: len(rows),
		Rows:           rows,
		LoadErrors:     loadErrors,
	}
	for _, r := range rows {
		if r.Status == string(VerdictDischarged) {
			report.Discharged++
		} else {
			report.Violations++
		}
	}
	return report
}

// Format renders a Report for terminal output.
func (s *ReportStage) Format(report *Report) string {
	out := fmt.Sprintf("  %d bridge call site%s: %d discharged, %d violation%s\n",
		report.TotalCallsites, plural(report.TotalCallsites),
		report.Discharged, report.Violations, plural(report.Violations))
	if len(report.LoadErrors) > 0 {
		out += fmt.Sprintf("  %d .proof load error%s:\n", len(report.LoadErrors), plural(len(report.LoadErrors)))
		for _, e := range report.LoadErrors {
			out += fmt.Sprintf("    %s: %s\n", e.ProofPath, e.Reason)
		}
	}
	for _, row := range report.Rows {
		if row.Status == string(VerdictDischarged) {
			continue
		}
		reason := ""
		if row.Reason != "" {
			reason = " — " + row.Reason
		}
		out += fmt.Sprintf("    ✗ %s in %s (%s…): %s%s\n",
			row.CallSite.BridgeIRName,
			row.CallSite.PropertyName,
			row.CallSite.PropertyCID[:12],
			row.Status,
			reason)
	}
	return out
}

func plural(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}
