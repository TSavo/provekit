.PHONY: all mint exhibit clean

# Top-level: mint catalog artifacts then run the exhibit
all: mint exhibit

# Step 1: mint the three catalog artifacts
mint:
	cd ../concept-shapes && python3 scripts/mint_option.py

# Step 2: run the exhibit driver (produces transport-report.json)
exhibit: artifacts/transport-report.json

artifacts/transport-report.json: driver.py source.rs
	python3 driver.py

# Clean generated artifacts (not catalog files)
clean:
	rm -f artifacts/rust_ir_term.json
	rm -f artifacts/concept_term.json
	rm -f artifacts/c_realized_term.json
	rm -f artifacts/composed_loss_record.json
	rm -f artifacts/transport-report.json
