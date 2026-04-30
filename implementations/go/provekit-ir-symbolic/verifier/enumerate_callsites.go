package verifier

// EnumerateCallsitesStage walks every property memento in the pool,
// finds Ctor terms whose name matches a bridge envelope's sourceSymbol,
// and emits CallSite records — one per bridge call discovered inside
// a property memento's IR formula.
type EnumerateCallsitesStage struct{}

// Run produces CallSite records.
func (s *EnumerateCallsitesStage) Run(pool *MementoPool) []CallSite {
	var out []CallSite
	for cid, env := range pool.Mementos {
		ev, ok := env["evidence"].(map[string]interface{})
		if !ok || ev["kind"] != "property" {
			continue
		}
		body, _ := ev["body"].(map[string]interface{})
		if body == nil {
			continue
		}
		formula, _ := body["irFormula"].(map[string]interface{})
		if formula == nil {
			continue
		}
		var propertyName string
		if scope, ok := body["scope"].(map[string]interface{}); ok {
			if name, ok := scope["name"].(string); ok {
				propertyName = name
			}
		}
		if propertyName == "" {
			propertyName = cid[:12] + "…"
		}
		s.walkFormula(formula, propertyName, cid, pool.BridgesBySymbol, &out)
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
	case "and":
		if conjuncts, ok := f["conjuncts"].([]interface{}); ok {
			for _, c := range conjuncts {
				if cm, ok := c.(map[string]interface{}); ok {
					s.walkFormula(cm, propertyName, propertyCID, bridges, out)
				}
			}
		}
	case "or":
		if disjuncts, ok := f["disjuncts"].([]interface{}); ok {
			for _, d := range disjuncts {
				if dm, ok := d.(map[string]interface{}); ok {
					s.walkFormula(dm, propertyName, propertyCID, bridges, out)
				}
			}
		}
	case "not":
		if body, ok := f["body"].(map[string]interface{}); ok {
			s.walkFormula(body, propertyName, propertyCID, bridges, out)
		}
	case "implies":
		if a, ok := f["antecedent"].(map[string]interface{}); ok {
			s.walkFormula(a, propertyName, propertyCID, bridges, out)
		}
		if c, ok := f["consequent"].(map[string]interface{}); ok {
			s.walkFormula(c, propertyName, propertyCID, bridges, out)
		}
	case "forall", "exists":
		if pred, ok := f["predicate"].(map[string]interface{}); ok {
			if body, ok := pred["body"].(map[string]interface{}); ok {
				s.walkFormula(body, propertyName, propertyCID, bridges, out)
			}
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
