package verifier

// EnumerateCallsitesStage walks every contract memento in the pool,
// finds Ctor terms whose name matches a bridge envelope's sourceSymbol,
// and emits CallSite records — one per bridge call discovered inside
// any of the contract's pre/post/inv slots.
//
// v1.1.0 IR shape consumed:
//
//	atomic     {kind:"atomic", name, args}
//	connective {kind:"and"|"or"|"not"|"implies", operands}
//	quantifier {kind:"forall"|"exists", name, sort, body}
//	var        {kind:"var", name}
//	ctor       {kind:"ctor", name, args}
type EnumerateCallsitesStage struct{}

// Run produces CallSite records.
func (s *EnumerateCallsitesStage) Run(pool *MementoPool) []CallSite {
	var out []CallSite
	for cid, env := range pool.Mementos {
		ev, ok := env["evidence"].(map[string]interface{})
		if !ok || ev["kind"] != "contract" {
			continue
		}
		body, _ := ev["body"].(map[string]interface{})
		if body == nil {
			continue
		}
		propertyName, _ := body["contractName"].(string)
		if propertyName == "" {
			propertyName = cid[:12] + "..."
		}
		// Walk pre/post/inv (whichever are present). Each can independently
		// contain ctor invocations of bridge-source symbols (call sites).
		if pre, ok := body["pre"].(map[string]interface{}); ok {
			s.walkFormula(pre, propertyName, cid, pool.BridgesBySymbol, &out)
		}
		if post, ok := body["post"].(map[string]interface{}); ok {
			s.walkFormula(post, propertyName, cid, pool.BridgesBySymbol, &out)
		}
		if inv, ok := body["inv"].(map[string]interface{}); ok {
			s.walkFormula(inv, propertyName, cid, pool.BridgesBySymbol, &out)
		}
	}
	return out
}

func (s *EnumerateCallsitesStage) walkFormula(
	f map[string]interface{},
	propertyName, propertyCID string,
	bridges map[string]map[string]interface{},
	out *[]CallSite,
) {
	switch f["kind"] {
	case "atomic":
		if args, ok := f["args"].([]interface{}); ok {
			for _, a := range args {
				if at, ok := a.(map[string]interface{}); ok {
					s.walkTerm(at, propertyName, propertyCID, bridges, out)
				}
			}
		}
	case "and", "or", "not", "implies":
		if operands, ok := f["operands"].([]interface{}); ok {
			for _, op := range operands {
				if om, ok := op.(map[string]interface{}); ok {
					s.walkFormula(om, propertyName, propertyCID, bridges, out)
				}
			}
		}
	case "forall", "exists":
		if body, ok := f["body"].(map[string]interface{}); ok {
			s.walkFormula(body, propertyName, propertyCID, bridges, out)
		}
	}
}

func (s *EnumerateCallsitesStage) walkTerm(
	t map[string]interface{},
	propertyName, propertyCID string,
	bridges map[string]map[string]interface{},
	out *[]CallSite,
) {
	if t["kind"] != "ctor" {
		return
	}
	name, _ := t["name"].(string)
	bridgeEnv, ok := bridges[name]
	if ok {
		ev := bridgeEnv["evidence"].(map[string]interface{})
		body := ev["body"].(map[string]interface{})
		args, _ := t["args"].([]interface{})
		var firstArg interface{}
		if len(args) > 0 {
			firstArg = args[0]
		}
		*out = append(*out, CallSite{
			BridgeIRName:      name,
			BridgeTargetCID:   asString(body["targetContractCid"]),
			BridgeSourceLayer: asString(body["sourceLayer"]),
			BridgeTargetLayer: asString(body["targetLayer"]),
			PropertyName:      propertyName,
			PropertyCID:       propertyCID,
			ArgTerm:           firstArg,
		})
	}
	// Recurse into ctor args.
	if args, ok := t["args"].([]interface{}); ok {
		for _, a := range args {
			if am, ok := a.(map[string]interface{}); ok {
				s.walkTerm(am, propertyName, propertyCID, bridges, out)
			}
		}
	}
}

func asString(v interface{}) string {
	if s, ok := v.(string); ok {
		return s
	}
	return ""
}
