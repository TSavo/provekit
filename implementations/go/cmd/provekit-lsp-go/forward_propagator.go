package main

import (
	"fmt"
	"sort"
	"strconv"
	"strings"

	canonicalizer "github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
)

const protocolCatalogCID = "blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f"

type Post struct {
	Constraints []string
	IsTop       bool
}

func EmptyPost() Post {
	return Post{}
}

func TopPost() Post {
	return Post{IsTop: true}
}

func (p Post) normalized() Post {
	if p.IsTop {
		return TopPost()
	}
	seen := map[string]bool{}
	for _, constraint := range p.Constraints {
		if constraint != "" {
			seen[constraint] = true
		}
	}
	constraints := make([]string, 0, len(seen))
	for constraint := range seen {
		constraints = append(constraints, constraint)
	}
	sort.Strings(constraints)
	return Post{Constraints: constraints}
}

func (p Post) combine(next Post) Post {
	if p.IsTop || next.IsTop {
		return TopPost()
	}
	combined := append(append([]string{}, p.Constraints...), next.Constraints...)
	return Post{Constraints: combined}.normalized()
}

func (p Post) branchMerge(other Post) Post {
	if p.IsTop || other.IsTop {
		return TopPost()
	}
	otherSet := map[string]bool{}
	for _, constraint := range other.Constraints {
		otherSet[constraint] = true
	}
	var shared []string
	for _, constraint := range p.Constraints {
		if otherSet[constraint] {
			shared = append(shared, constraint)
		}
	}
	return Post{Constraints: shared}.normalized()
}

func (p Post) cid() string {
	if p.IsTop {
		return cidForBytes([]byte("post:top"))
	}
	normalized := p.normalized()
	return cidForBytes([]byte("post:known:" + strings.Join(normalized.Constraints, "\n")))
}

type ForwardStmtKind string

const (
	ForwardStmtReset       ForwardStmtKind = "reset"
	ForwardStmtAssign      ForwardStmtKind = "assign"
	ForwardStmtCall        ForwardStmtKind = "call"
	ForwardStmtIfElse      ForwardStmtKind = "if_else"
	ForwardStmtUnsupported ForwardStmtKind = "unsupported"
)

type ForwardStmt struct {
	Kind       ForwardStmtKind
	Post       Post
	CalleeID   string
	Range      LSPRange
	ThenBranch []ForwardStmt
	ElseBranch []ForwardStmt
}

type LSPPosition struct {
	Line      int `json:"line"`
	Character int `json:"character"`
}

type LSPRange struct {
	Start LSPPosition `json:"start"`
	End   LSPPosition `json:"end"`
}

func SingleLineRange(line, startCharacter, endCharacter int) LSPRange {
	return LSPRange{
		Start: LSPPosition{Line: line, Character: startCharacter},
		End:   LSPPosition{Line: line, Character: endCharacter},
	}
}

type BaselineEntry struct {
	CalleeID               string
	Pre                    *Post
	Post                   *Post
	ContractName           string
	MemberCID              string
	ContractCID            string
	AttestationCID         string
	PreCID                 string
	PostCID                string
	Signer                 string
	SignerRole             string
	BaselineCatalogCID     string
	BaselineContractSetCID string
	BaselineIndexCID       string
	ProtocolCatalogCID     string
}

func NewBaselineEntry(calleeID string, pre *Post, post *Post) BaselineEntry {
	preCID := cidForBytes([]byte(calleeID + ":pre:none"))
	if pre != nil {
		normalized := pre.normalized()
		pre = &normalized
		preCID = normalized.cid()
	}
	postCID := cidForBytes([]byte(calleeID + ":post:none"))
	if post != nil {
		normalized := post.normalized()
		post = &normalized
		postCID = normalized.cid()
	}
	seed := calleeID + "|" + preCID + "|" + postCID
	return BaselineEntry{
		CalleeID:               calleeID,
		Pre:                    pre,
		Post:                   post,
		ContractName:           "go_baseline_" + sanitizeIdentifier(calleeID),
		MemberCID:              cidForBytes([]byte("member:" + seed)),
		ContractCID:            cidForBytes([]byte("contract:" + seed)),
		AttestationCID:         cidForBytes([]byte("attestation:" + seed)),
		PreCID:                 preCID,
		PostCID:                postCID,
		Signer:                 "ed25519:foundation-v0",
		SignerRole:             "foundation-baseline",
		BaselineCatalogCID:     cidForBytes([]byte("baseline-catalog:" + seed)),
		BaselineContractSetCID: cidForBytes([]byte("baseline-contract-set:" + seed)),
		BaselineIndexCID:       cidForBytes([]byte("baseline-index:" + seed)),
		ProtocolCatalogCID:     protocolCatalogCID,
	}
}

type DiagnosticData struct {
	SchemaVersion          int      `json:"schema_version"`
	Kind                   string   `json:"kind"`
	Callee                 string   `json:"callee"`
	CalleeContractCID      string   `json:"callee_contract_cid"`
	CalleeAttestationCID   string   `json:"callee_attestation_cid"`
	CalleePreCID           string   `json:"callee_pre_cid"`
	CalleePostCID          string   `json:"callee_post_cid"`
	CurrentPostCID         string   `json:"current_post_cid"`
	MissingConjuncts       []string `json:"missing_conjuncts"`
	Signer                 string   `json:"signer"`
	SignerRole             string   `json:"signer_role"`
	BaselineCatalogCID     string   `json:"baseline_catalog_cid"`
	BaselineContractSetCID string   `json:"baseline_contract_set_cid"`
	BaselineIndexCID       string   `json:"baseline_index_cid"`
	ProtocolCatalogCID     string   `json:"protocol_catalog_cid"`
}

type LSPDiagnostic struct {
	Range    LSPRange       `json:"range"`
	Severity int            `json:"severity"`
	Source   string         `json:"source"`
	Code     string         `json:"code"`
	Message  string         `json:"message"`
	Data     DiagnosticData `json:"data"`
}

type ForwardPropagator struct {
	index map[string]BaselineEntry
}

func NewForwardPropagator(entries []BaselineEntry) ForwardPropagator {
	index := map[string]BaselineEntry{}
	for _, entry := range entries {
		index[entry.CalleeID] = entry
	}
	return ForwardPropagator{index: index}
}

func FloorV1SeedForwardPropagator() ForwardPropagator {
	pre := Post{Constraints: []string{"x > 0"}}
	post := Post{Constraints: []string{"returns true"}}
	return NewForwardPropagator([]BaselineEntry{
		NewBaselineEntry("checkPositive", &pre, &post),
	})
}

func (p ForwardPropagator) EmitDiagnostics(body []ForwardStmt) []LSPDiagnostic {
	diagnostics := []LSPDiagnostic{}
	p.walkBlock(body, EmptyPost(), &diagnostics)
	return diagnostics
}

func (p ForwardPropagator) CheckCallsite(calleeID string, currentPost Post, lspRange LSPRange) *LSPDiagnostic {
	if currentPost.IsTop {
		return nil
	}
	entry, ok := p.index[calleeID]
	if !ok || entry.Pre == nil {
		return nil
	}
	currentSet := map[string]bool{}
	for _, constraint := range currentPost.normalized().Constraints {
		currentSet[constraint] = true
	}
	var missing []string
	for _, constraint := range entry.Pre.normalized().Constraints {
		if !currentSet[constraint] {
			missing = append(missing, constraint)
		}
	}
	if len(missing) == 0 {
		return nil
	}
	return &LSPDiagnostic{
		Range:    lspRange,
		Severity: 1,
		Source:   "provekit",
		Code:     "implication-failed",
		Message:  "callee precondition not established at this callsite",
		Data: DiagnosticData{
			SchemaVersion:          1,
			Kind:                   "provekit.lsp.implication_failed",
			Callee:                 entry.CalleeID,
			CalleeContractCID:      entry.ContractCID,
			CalleeAttestationCID:   entry.AttestationCID,
			CalleePreCID:           entry.PreCID,
			CalleePostCID:          entry.PostCID,
			CurrentPostCID:         currentPost.cid(),
			MissingConjuncts:       missing,
			Signer:                 entry.Signer,
			SignerRole:             entry.SignerRole,
			BaselineCatalogCID:     entry.BaselineCatalogCID,
			BaselineContractSetCID: entry.BaselineContractSetCID,
			BaselineIndexCID:       entry.BaselineIndexCID,
			ProtocolCatalogCID:     entry.ProtocolCatalogCID,
		},
	}
}

func (p ForwardPropagator) walkBlock(body []ForwardStmt, startPost Post, diagnostics *[]LSPDiagnostic) Post {
	currentPost := startPost
	for _, stmt := range body {
		switch stmt.Kind {
		case ForwardStmtReset:
			currentPost = EmptyPost()
		case ForwardStmtAssign:
			currentPost = currentPost.combine(stmt.Post)
		case ForwardStmtCall:
			diagnostic := p.CheckCallsite(stmt.CalleeID, currentPost, stmt.Range)
			if diagnostic != nil {
				*diagnostics = append(*diagnostics, *diagnostic)
				break
			}
			if entry, ok := p.index[stmt.CalleeID]; ok {
				if entry.Post != nil {
					currentPost = currentPost.combine(*entry.Post)
				}
			} else {
				currentPost = TopPost()
			}
		case ForwardStmtIfElse:
			thenPost := p.walkBlock(stmt.ThenBranch, currentPost, diagnostics)
			elsePost := p.walkBlock(stmt.ElseBranch, currentPost, diagnostics)
			currentPost = thenPost.branchMerge(elsePost)
		case ForwardStmtUnsupported:
			currentPost = TopPost()
		}
	}
	return currentPost
}

func LowerFloorSource(source string) []ForwardStmt {
	stmts := []ForwardStmt{}
	braceDepth := 0
	var topBlockDepth *int

	for lineIdx, line := range strings.Split(source, "\n") {
		trimmed := strings.TrimLeft(line, " \t")
		isFunctionDefinition := isGoFunctionHeader(trimmed)
		if isFunctionDefinition {
			stmts = append(stmts, ForwardStmt{Kind: ForwardStmtReset})
			topBlockDepth = nil
		}

		if startsTopFallbackBlock(trimmed) {
			depth := braceDepth + strings.Count(line, "{") - strings.Count(line, "}")
			if depth == braceDepth {
				depth = braceDepth + 1
			}
			topBlockDepth = &depth
		}

		if !isFunctionDefinition {
			for _, call := range checkPositiveCalls(line) {
				if topBlockDepth != nil {
					stmts = append(stmts, ForwardStmt{Kind: ForwardStmtUnsupported})
				} else {
					stmts = append(stmts, ForwardStmt{
						Kind: ForwardStmtAssign,
						Post: postForCheckPositiveArg(call.arg),
					})
				}
				stmts = append(stmts, ForwardStmt{
					Kind:     ForwardStmtCall,
					CalleeID: "checkPositive",
					Range:    SingleLineRange(lineIdx, call.start, call.start+len("checkPositive")),
				})
			}
		}

		braceDepth += strings.Count(line, "{")
		braceDepth -= strings.Count(line, "}")
		if topBlockDepth != nil && braceDepth < *topBlockDepth {
			topBlockDepth = nil
		}
	}

	return stmts
}

func isGoFunctionHeader(trimmed string) bool {
	if !strings.HasPrefix(trimmed, "func") {
		return false
	}
	if len(trimmed) == len("func") || (trimmed[len("func")] != ' ' && trimmed[len("func")] != '\t') {
		return false
	}
	rest := strings.TrimLeft(trimmed[len("func"):], " \t")
	return rest != ""
}

type checkPositiveCall struct {
	start int
	arg   string
}

func checkPositiveCalls(line string) []checkPositiveCall {
	calls := []checkPositiveCall{}
	searchFrom := 0
	for {
		relativeStart := strings.Index(line[searchFrom:], "checkPositive(")
		if relativeStart < 0 {
			break
		}
		start := searchFrom + relativeStart
		argsStart := start + len("checkPositive(")
		relativeEnd := strings.Index(line[argsStart:], ")")
		if relativeEnd < 0 {
			break
		}
		end := argsStart + relativeEnd
		calls = append(calls, checkPositiveCall{start: start, arg: strings.TrimSpace(line[argsStart:end])})
		searchFrom = end + 1
	}
	return calls
}

func startsTopFallbackBlock(trimmed string) bool {
	return strings.HasPrefix(trimmed, "for ") || strings.HasPrefix(trimmed, "for{")
}

func postForCheckPositiveArg(arg string) Post {
	value, err := strconv.Atoi(arg)
	if err != nil {
		return TopPost()
	}
	if value > 0 {
		return Post{Constraints: []string{"x > 0"}}
	}
	return Post{Constraints: []string{"x <= 0"}}
}

func cidForBytes(bytes []byte) string {
	return canonicalizer.ComputeCID(bytes)
}

func sanitizeIdentifier(value string) string {
	var b strings.Builder
	for _, r := range value {
		if (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
		} else {
			b.WriteRune('_')
		}
	}
	if b.Len() == 0 {
		return "unknown"
	}
	return b.String()
}

func (d DiagnosticData) String() string {
	return fmt.Sprintf("%s %v", d.Callee, d.MissingConjuncts)
}
