#!/usr/bin/env bash
# verify.sh — verify the architectural-assembly authorship attestation.
#
# Three independent checks. Each can be skipped if the underlying tool
# isn't installed, but a fully-passing run requires all three.
#
# Usage: ./provenance/v1/verify.sh

set -uo pipefail

# Colors when stdout is a terminal.
if [ -t 1 ]; then
    RED=$'\033[0;31m'; GREEN=$'\033[0;32m'; YELLOW=$'\033[1;33m'; NC=$'\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; NC=''
fi

DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
UMBRELLA="$DIR/umbrella.json"
ATTEST="$DIR/attestation.json"
PUBKEY="$DIR/pubkey.txt"
TXID_FILE="$DIR/anchor-bitcoin-txid.txt"
OTS_FILE="$DIR/attestation.json.ots"

PASS=0; FAIL=0; SKIP=0
ok()   { echo "${GREEN}✓${NC} $1"; PASS=$((PASS+1)); }
bad()  { echo "${RED}✗${NC} $1"; FAIL=$((FAIL+1)); }
skip() { echo "${YELLOW}~${NC} $1"; SKIP=$((SKIP+1)); }

GPG_PUBKEY="$DIR/pubkey.gpg"
GPG_SIG="$DIR/attestation.json.asc"

# -----------------------------------------------------------------------------
# Check 1: umbrella CID matches the recomputed value over the manifest entries
# -----------------------------------------------------------------------------
echo "[1/4] Recomputing umbrella CID from manifest entries"

if ! command -v python3 >/dev/null 2>&1; then
    skip "python3 not on PATH; cannot recompute umbrella CID"
elif ! python3 -c "import blake3" >/dev/null 2>&1; then
    skip "python3 -m blake3 not installed (try: pip install blake3)"
else
    expected=$(python3 -c "import json,sys; print(json.load(open('$UMBRELLA'))['umbrellaCid'])")
    observed=$(python3 - "$ROOT" "$UMBRELLA" <<'PY'
import sys, json, blake3
root, umbrella_path = sys.argv[1], sys.argv[2]
manifest = json.load(open(umbrella_path))
cids = []
for entry in manifest["entries"]:
    p = root + "/" + entry["path"]
    with open(p, "rb") as f:
        d = blake3.blake3(f.read(), max_threads=blake3.blake3.AUTO).digest(length=64)
    cid = "blake3-512:" + d.hex()
    if cid != entry["cid"]:
        print("MISMATCH on " + entry["path"], file=sys.stderr)
        sys.exit(2)
    cids.append(cid)
cids.sort()
jcs = json.dumps(cids, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
umbrella = blake3.blake3(jcs, max_threads=blake3.blake3.AUTO).digest(length=64)
print("blake3-512:" + umbrella.hex())
PY
    )
    rc=$?
    if [ $rc -ne 0 ]; then
        bad "umbrella recomputation failed (one or more file CIDs do not match)"
    elif [ "$expected" = "$observed" ]; then
        ok "umbrella CID matches: $expected"
    else
        bad "umbrella CID drift: expected $expected, observed $observed"
    fi
fi

# -----------------------------------------------------------------------------
# Check 2: attestation Ed25519 signature
# -----------------------------------------------------------------------------
echo "[2/4] Verifying Ed25519 signature on attestation.json"

if [ ! -s "$PUBKEY" ]; then
    skip "pubkey.txt missing or empty; signature ceremony not yet completed"
else
    sig=$(python3 -c "import json; print(json.load(open('$ATTEST')).get('signature',''))")
    if [ -z "$sig" ] || [[ "$sig" == \<* ]]; then
        skip "attestation.signature still a placeholder"
    elif ! python3 -c "import nacl.signing" >/dev/null 2>&1; then
        skip "PyNaCl not installed (try: pip install pynacl)"
    else
        result=$(python3 - "$ATTEST" "$PUBKEY" <<'PY'
import sys, json, base64
from nacl.signing import VerifyKey
from nacl.exceptions import BadSignatureError

attest_path, pubkey_path = sys.argv[1], sys.argv[2]
attest = json.load(open(attest_path))
pubkey_line = open(pubkey_path).read().strip()
if pubkey_line.startswith("ed25519:"):
    pubkey_line = pubkey_line[len("ed25519:"):]
pubkey = base64.b64decode(pubkey_line)

sig_field = attest["signature"]
if sig_field.startswith("ed25519:"):
    sig_field = sig_field[len("ed25519:"):]
sig = base64.b64decode(sig_field)

# Build the signed message: JCS-encoded attestation minus signature + _signing_instructions
msg_dict = {k: v for k, v in attest.items() if k not in ("signature", "_signing_instructions")}
# JCS sorts keys alphabetically and uses compact separators
def jcs(obj):
    if isinstance(obj, dict):
        items = sorted(obj.items())
        return "{" + ",".join(f"{json.dumps(k,ensure_ascii=False)}:{jcs(v)}" for k,v in items) + "}"
    elif isinstance(obj, list):
        return "[" + ",".join(jcs(x) for x in obj) + "]"
    else:
        return json.dumps(obj, ensure_ascii=False, separators=(",",":"))

msg = jcs(msg_dict).encode("utf-8")

try:
    VerifyKey(pubkey).verify(msg, sig)
    print("OK")
except BadSignatureError:
    print("BAD")
PY
        )
        case "$result" in
            OK)  ok "Ed25519 signature verifies under pubkey.txt" ;;
            BAD) bad "Ed25519 signature does NOT verify (bad signature or wrong pubkey)" ;;
            *)   bad "signature verification raised an unexpected error: $result" ;;
        esac
    fi
fi

# -----------------------------------------------------------------------------
# Check 3: GPG detached signature over attestation.json (independent of Ed25519)
# -----------------------------------------------------------------------------
echo "[3/4] Verifying GPG detached signature on attestation.json"

if [ ! -f "$GPG_PUBKEY" ] || [ ! -f "$GPG_SIG" ]; then
    skip "pubkey.gpg or attestation.json.asc missing; GPG ceremony not yet completed"
elif ! command -v gpg >/dev/null 2>&1; then
    skip "gpg not on PATH; cannot verify detached signature"
else
    # Import the architect's pubkey into a throwaway keyring so verification
    # is independent of the verifier's existing keyring state.
    tmp_gpg_home=$(mktemp -d)
    trap "rm -rf '$tmp_gpg_home'" EXIT
    if gpg --homedir "$tmp_gpg_home" --quiet --batch --import "$GPG_PUBKEY" 2>/dev/null \
       && gpg --homedir "$tmp_gpg_home" --quiet --batch --verify "$GPG_SIG" "$ATTEST" 2>/dev/null; then
        fpr=$(gpg --homedir "$tmp_gpg_home" --list-keys --with-fingerprint --with-colons 2>/dev/null \
               | awk -F: '/^fpr:/ {print $10; exit}')
        ok "GPG signature verifies (fingerprint $fpr)"
    else
        bad "GPG signature does NOT verify (bad signature, wrong key, or malformed pubkey)"
    fi
fi

# -----------------------------------------------------------------------------
# Check 4: Bitcoin OP_RETURN anchor (or OpenTimestamps)
# -----------------------------------------------------------------------------
echo "[4/4] Verifying public time anchor"

if [ -s "$TXID_FILE" ]; then
    txid=$(tr -d '[:space:]' < "$TXID_FILE")
    if [ ${#txid} -ne 64 ]; then
        bad "anchor-bitcoin-txid.txt does not contain a 64-char hex txid (got ${#txid} chars)"
    elif command -v curl >/dev/null 2>&1; then
        # Fetch the transaction via a public block explorer; assert the OP_RETURN
        # contains the attestation CID (computed from attestation.json).
        attest_cid=$(python3 -c "
import json, blake3
data = open('$ATTEST', 'rb').read()
print('blake3-512:' + blake3.blake3(data, max_threads=blake3.blake3.AUTO).digest(length=64).hex())
" 2>/dev/null)
        if [ -z "$attest_cid" ]; then
            skip "could not compute attestation CID (python3 + blake3 required); cannot verify OP_RETURN payload"
        else
            # mempool.space is the most-cited public Bitcoin explorer with no API key
            tx_json=$(curl -fsS "https://mempool.space/api/tx/$txid" 2>/dev/null || true)
            if [ -z "$tx_json" ]; then
                skip "could not fetch txid from mempool.space (network or txid unknown to mempool)"
            else
                # OP_RETURN scripts encode 6a [len] [data]; find any vout with scriptpubkey starting with 6a
                op_return_data=$(echo "$tx_json" | python3 -c "
import sys, json
tx = json.load(sys.stdin)
for v in tx.get('vout', []):
    asm = v.get('scriptpubkey_asm','')
    if asm.startswith('OP_RETURN'):
        # asm is like 'OP_RETURN OP_PUSHBYTES_64 <hex>'
        parts = asm.split()
        if len(parts) >= 3:
            print(parts[-1])
            break
" 2>/dev/null)
                expected_hex=${attest_cid#blake3-512:}
                if [ "$op_return_data" = "$expected_hex" ]; then
                    ok "Bitcoin OP_RETURN at txid $txid encodes attestation CID $attest_cid"
                else
                    bad "Bitcoin OP_RETURN payload does not match attestation CID (got '$op_return_data', expected '$expected_hex')"
                fi
            fi
        fi
    else
        skip "curl not on PATH; cannot fetch Bitcoin tx for verification"
    fi
elif [ -s "$OTS_FILE" ]; then
    if command -v ots >/dev/null 2>&1; then
        ots_out=$(ots verify "$OTS_FILE" 2>&1)
        if echo "$ots_out" | grep -q "Success"; then
            ok "OpenTimestamps proof verifies (anchor in Bitcoin via OTS calendar)"
        elif echo "$ots_out" | grep -qE "Pending|incomplete|not complete"; then
            # Pending: stamp submitted but not yet aggregated into a Bitcoin block.
            # Run `ots upgrade attestation.json.ots` after a few hours and retry.
            skip "OpenTimestamps proof pending Bitcoin confirmation (run: ots upgrade $OTS_FILE)"
        else
            bad "ots verify failed; calendar unreachable or proof corrupt"
        fi
    else
        skip "ots CLI not installed; cannot verify .ots file"
    fi
else
    skip "no Bitcoin txid and no .ots file present; ceremony not yet completed"
fi

# -----------------------------------------------------------------------------
echo ""
echo "==============================="
printf "Results: ${GREEN}%d pass${NC}, ${RED}%d fail${NC}, ${YELLOW}%d skip${NC}\n" "$PASS" "$FAIL" "$SKIP"
echo "==============================="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
