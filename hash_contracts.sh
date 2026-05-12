#!/bin/bash
set -eu

# Generate and hash the three bool_cell C contracts
python3 mint_bool_cell.py > /tmp/all_contracts.txt

# Extract each contract individually and compute its blake3 hash
# Contract 1: c:bool_cell_get
python3 -c "
import json
import sys
import subprocess

# Generate all contracts
exec(open('mint_bool_cell.py').read().replace('if __name__', 'if False'))

contract = bool_cell_c_get_contract()
json_str = json.dumps(contract, separators=(',', ':'), sort_keys=True)
print(json_str, end='', file=sys.stderr)
print(json_str)
" 2>/dev/null | blake3 --raw | xxd -p

echo "bool_cell_c_get generated"
