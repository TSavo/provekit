// SPDX-License-Identifier: Apache-2.0
//
// Three-tier discharge benchmark.
//
// Loads a previously-generated lattice into a hash-into-pool index
// (hash -> implication CID). Picks Q random handshake queries and
// runs each through three tiers:
//
//   Tier 1: BLAKE3-512 equality on the 64-byte digest
//           (the bare "is the consumer's pre the same hash as the
//            publisher's post" check).
//
//   Tier 2: hash-into-pool lookup of an implication memento + an
//           ed25519 signature verification of the implication's
//           producer signature. This is the cached-implication
//           cost: the lattice answers the query in one map lookup
//           plus one signature-verify.
//
//   Tier 3: solver-from-scratch. We shell `z3 -in` with a small but
//           non-trivial SMT-LIB query. This stands in for the cost
//           the solver would pay if no cached implication existed.
//
// We keep the bench dependency-free of provekit-verifier on purpose:
// the verifier crate is being upgraded by a parallel agent. The
// numbers we report here are wall-clock costs of the underlying
// primitives, which is what the showcase claim is about.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;
use walkdir::WalkDir;

use provekit_canonicalizer::Value;
use provekit_proof_envelope::ed25519_verify_string;

use crate::BenchmarkArgs;

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("lattice: {0}")]
    Lattice(String),
    #[error("z3 not runnable at {path:?}: {source}")]
    Z3Spawn {
        path: String,
        source: std::io::Error,
    },
}

/// In-memory record describing one implication memento we loaded.
#[derive(Debug, Clone)]
struct ImpRecord {
    #[allow(dead_code)] // captured for diagnostic/debugging; bench currently uses hashes only
    cid: String,
    antecedent_hash: String,
    consequent_hash: String,
    producer_id: String,
    producer_signature: String,
    /// Bytes covered by the producer signature (the canonical
    /// envelope minus cid + producerSignature). For .proof bundles
    /// the inner member bytes are JCS-canonical envelope bytes; we
    /// reconstruct the unsigned-canonical bytes by stripping the cid
    /// and producerSignature fields, re-emitting JCS, and signing
    /// that. To keep the bench self-contained we store the full
    /// JCS-canonical inner envelope bytes here and re-derive the
    /// signed-payload bytes on demand.
    envelope_bytes: Vec<u8>,
}

/// Pool of every implication memento, indexed by antecedent hash so
/// we can answer "does there exist a cached implication from
/// antecedent A to consequent B?" in O(1).
struct LatticePool {
    /// Antecedent-hash -> list of implication record indices.
    by_antecedent: HashMap<String, Vec<usize>>,
    impls: Vec<ImpRecord>,
    /// All publisher-post hashes we observed in any contract: used
    /// as the source of "post" sample values.
    post_hashes: Vec<String>,
    /// All consumer-pre hashes we observed in any contract: used as
    /// the source of "pre" sample values.
    pre_hashes: Vec<String>,
    /// Total bytes on disk.
    on_disk_bytes: u64,
    /// Mementos counted (every .proof file).
    proof_files: usize,
}

pub fn run(args: &BenchmarkArgs) -> Result<(), BenchError> {
    eprintln!(
        "provekit-showcase: benchmark  lattice={} queries={}",
        args.lattice.display(),
        args.queries
    );
    let t_load = Instant::now();
    let pool = load_lattice(&args.lattice)?;
    eprintln!(
        "  loaded {} .proof files in {:.2}s ({} implications)",
        pool.proof_files,
        t_load.elapsed().as_secs_f64(),
        pool.impls.len()
    );

    if pool.impls.is_empty() {
        return Err(BenchError::Lattice("no implications loaded".into()));
    }
    if pool.pre_hashes.is_empty() || pool.post_hashes.is_empty() {
        return Err(BenchError::Lattice(
            "no contract pre/post hashes observed".into(),
        ));
    }

    // Verify z3 is present.
    let z3_present = Command::new(&args.z3_path)
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !z3_present {
        eprintln!("WARNING: z3 at {:?} not runnable; tier-3 timings will be skipped (use --z3-path or PROVEKIT_Z3 to point at a real binary)", args.z3_path);
    }

    let q = args.queries;
    let mut t1: Vec<u128> = Vec::with_capacity(q);
    let mut t2: Vec<u128> = Vec::with_capacity(q);
    let mut t3: Vec<u128> = Vec::with_capacity(q);

    let mut t2_hits = 0usize;
    let mut t3_runs = 0usize;
    // Tier 3 is much slower; we sample at most this many runs to
    // stay within the time budget. Default ~64 runs gives a stable
    // median to within a few percent.
    let tier3_sample = q.min(64);

    let mut state = args.seed.wrapping_mul(0x9E3779B97F4A7C15);

    // Half the queries are guaranteed cache-hits (we sample an
    // existing implication and ask its (antecedent, consequent)
    // pair). The other half are random: these measure the
    // tier-2 negative-lookup cost (hash-into-pool miss). The
    // mix gives an honest overall median.
    let imp_keys: Vec<(&String, &String)> = pool
        .impls
        .iter()
        .map(|r| (&r.antecedent_hash, &r.consequent_hash))
        .collect();

    for i in 0..q {
        let (pre, post): (&String, &String) = if i % 2 == 0 && !imp_keys.is_empty() {
            // Guaranteed hit: pick a real (antecedent, consequent)
            // pair from the implication set. consumer-pre = pair.1
            // (the consequent slot), publisher-post = pair.0 (the
            // antecedent slot).
            let k = lcg_pick(&mut state, &imp_keys);
            (k.1, k.0)
        } else {
            let pre = lcg_pick(&mut state, &pool.pre_hashes);
            let post = lcg_pick(&mut state, &pool.post_hashes);
            (pre, post)
        };

        // ----- Tier 1: BLAKE3-512 equality on the 64-byte digest. -----
        // We compare the 128-hex-byte string forms (which is what
        // every CID in the lattice carries). To keep the timing
        // honest about "memcmp(64)" we slice off the prefix and
        // compare the digest bytes directly.
        let pre_d = digest_bytes(pre);
        let post_d = digest_bytes(post);
        let t = Instant::now();
        let _eq = ct_eq_64(&pre_d, &post_d);
        t1.push(t.elapsed().as_nanos());

        // ----- Tier 2: hash-into-pool implication lookup + verify. ----
        let t = Instant::now();
        let mut found = false;
        if let Some(idxs) = pool.by_antecedent.get(post) {
            // Walk the bucket; verify the first one whose
            // consequent matches.
            for ix in idxs.iter() {
                let r = &pool.impls[*ix];
                if r.consequent_hash == *pre {
                    // Verify the producer's signature over the inner
                    // canonical envelope. We rebuild the unsigned
                    // bytes by stripping cid + producerSignature
                    // fields from the JCS-canonical envelope.
                    if verify_envelope_signature(r) {
                        found = true;
                        break;
                    }
                }
            }
        }
        t2.push(t.elapsed().as_nanos());
        if found {
            t2_hits += 1;
        }

        // ----- Tier 3: Z3 from scratch (sampled). -----
        if z3_present && i < tier3_sample {
            let smt = synth_smt(pre, post, i as u64);
            let t = Instant::now();
            let _ = run_z3(&args.z3_path, &smt)?;
            t3.push(t.elapsed().as_nanos());
            t3_runs += 1;
        }

        if (i + 1) % (q / 10).max(1) == 0 {
            eprintln!("  query {} / {}", i + 1, q);
        }
    }

    let summary = Summary {
        queries: q,
        on_disk_bytes: pool.on_disk_bytes,
        proof_files: pool.proof_files,
        implications: pool.impls.len(),
        tier1: percentiles(&mut t1),
        tier2: percentiles(&mut t2),
        tier2_hits: t2_hits,
        tier3: if t3.is_empty() {
            Percentiles::default()
        } else {
            percentiles(&mut t3)
        },
        tier3_runs: t3_runs,
        z3_present,
    };
    println!("{}", summary.render());
    Ok(())
}

#[derive(Debug, Default, Clone, Copy)]
struct Percentiles {
    p10_ns: u128,
    p50_ns: u128,
    p90_ns: u128,
    min_ns: u128,
    max_ns: u128,
    mean_ns: u128,
    count: usize,
}

fn percentiles(samples: &mut Vec<u128>) -> Percentiles {
    if samples.is_empty() {
        return Percentiles::default();
    }
    samples.sort_unstable();
    let n = samples.len();
    let pick = |p: f64| -> u128 {
        let idx = ((n as f64 - 1.0) * p).round() as usize;
        samples[idx]
    };
    let sum: u128 = samples.iter().sum();
    Percentiles {
        p10_ns: pick(0.10),
        p50_ns: pick(0.50),
        p90_ns: pick(0.90),
        min_ns: samples[0],
        max_ns: samples[n - 1],
        mean_ns: sum / n as u128,
        count: n,
    }
}

struct Summary {
    queries: usize,
    on_disk_bytes: u64,
    proof_files: usize,
    implications: usize,
    tier1: Percentiles,
    tier2: Percentiles,
    tier2_hits: usize,
    tier3: Percentiles,
    tier3_runs: usize,
    z3_present: bool,
}

impl Summary {
    fn render(&self) -> String {
        let mut s = String::new();
        s.push_str("provekit-showcase benchmark report\n");
        s.push_str("==================================\n\n");
        s.push_str(&format!(
            "lattice:\n  proof_files:    {}\n  implications:   {}\n  on_disk_bytes:  {} ({:.2} MiB)\n\n",
            self.proof_files,
            self.implications,
            self.on_disk_bytes,
            self.on_disk_bytes as f64 / (1024.0 * 1024.0),
        ));
        s.push_str(&format!("queries:           {}\n", self.queries));
        s.push_str(&format!(
            "tier2_hit_rate:    {} / {} ({:.2}%)\n\n",
            self.tier2_hits,
            self.queries,
            100.0 * self.tier2_hits as f64 / self.queries.max(1) as f64
        ));
        s.push_str("tier 1  hash-equality (memcmp of 64 bytes)\n");
        s.push_str(&render_pct(&self.tier1));
        s.push_str("tier 2  cached-implication lookup + ed25519 verify\n");
        s.push_str(&render_pct(&self.tier2));
        if self.z3_present && self.tier3.count > 0 {
            s.push_str(&format!(
                "tier 3  z3 from scratch (sampled, {} runs)\n",
                self.tier3_runs
            ));
            s.push_str(&render_pct(&self.tier3));
        } else {
            s.push_str("tier 3  z3 from scratch: SKIPPED (z3 not runnable)\n\n");
        }
        // Compression ratio: any one query is verified by 64
        // (digest) bytes; the total lattice on disk is on_disk_bytes.
        let ratio = self.on_disk_bytes as f64 / 64.0;
        s.push_str(&format!(
            "compression:\n  64 bytes per query   |   {} bytes on disk   |   ratio = {:.2e}\n",
            self.on_disk_bytes, ratio
        ));
        s
    }
}

fn render_pct(p: &Percentiles) -> String {
    if p.count == 0 {
        return "  (no samples)\n\n".to_string();
    }
    format!(
        "  count={} min={} p10={} p50={} p90={} max={} mean={}  (ns)\n  {}\n\n",
        p.count,
        p.min_ns,
        p.p10_ns,
        p.p50_ns,
        p.p90_ns,
        p.max_ns,
        p.mean_ns,
        humanize(p.p50_ns)
    )
}

fn humanize(ns: u128) -> String {
    if ns < 1_000 {
        format!("p50 = {} ns", ns)
    } else if ns < 1_000_000 {
        format!("p50 = {:.2} us", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("p50 = {:.2} ms", ns as f64 / 1_000_000.0)
    } else {
        format!("p50 = {:.2} s", ns as f64 / 1_000_000_000.0)
    }
}

// ---------------------------------------------------------------------------
// Lattice loader
// ---------------------------------------------------------------------------

fn load_lattice(root: &Path) -> Result<LatticePool, BenchError> {
    let mut entries: Vec<PathBuf> = Vec::new();
    let mut on_disk_bytes: u64 = 0;
    for e in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !e.file_type().is_file() {
            continue;
        }
        let p = e.into_path();
        if p.extension().and_then(|x| x.to_str()) != Some("proof") {
            continue;
        }
        on_disk_bytes += p.metadata().map(|m| m.len()).unwrap_or(0);
        entries.push(p);
    }

    // Parse CBOR proof envelopes in parallel; for each member we
    // discover, route to the right bucket.
    let parsed: Vec<ParseResult> = entries
        .par_iter()
        .filter_map(|p| match parse_proof_file(p) {
            Ok(r) => Some(r),
            Err(_) => None,
        })
        .collect();

    let mut impls: Vec<ImpRecord> = Vec::new();
    let mut by_antecedent: HashMap<String, Vec<usize>> = HashMap::new();
    let mut post_hashes: Vec<String> = Vec::new();
    let mut pre_hashes: Vec<String> = Vec::new();
    for r in parsed {
        match r {
            ParseResult::Implication(rec) => {
                let ix = impls.len();
                by_antecedent
                    .entry(rec.antecedent_hash.clone())
                    .or_default()
                    .push(ix);
                impls.push(rec);
            }
            ParseResult::Contract {
                pre_hash,
                post_hash,
            } => {
                pre_hashes.push(pre_hash);
                post_hashes.push(post_hash);
            }
            ParseResult::Other => {}
        }
    }

    Ok(LatticePool {
        by_antecedent,
        impls,
        post_hashes,
        pre_hashes,
        on_disk_bytes,
        proof_files: entries.len(),
    })
}

enum ParseResult {
    Implication(ImpRecord),
    Contract { pre_hash: String, post_hash: String },
    Other,
}

/// Parse a .proof CBOR envelope and inspect its inner member.
/// Members are one entry whose value is JCS-canonical JSON bytes; we
/// scan that JSON for the fields we need (kind, antecedentHash,
/// consequentHash, preHash, postHash).
fn parse_proof_file(path: &Path) -> Result<ParseResult, BenchError> {
    let bytes = std::fs::read(path)?;
    // The .proof CBOR envelope has shape { kind: "catalog", ...,
    // members: { <cid>: <bstr-of-JCS-bytes>, ... } }. We only need
    // the inner JCS-JSON bytes; pull them out by minimal scanning.
    let inner = extract_first_member(&bytes)
        .ok_or_else(|| BenchError::Lattice(format!("no member in {}", path.display())))?;

    // Parse inner JSON enough to read the fields we care about.
    let s = std::str::from_utf8(inner).map_err(|e| BenchError::Lattice(e.to_string()))?;

    // Detect role by direct substring match on the canonical kind
    // marker. We search for the literal pair `"kind":"implication"`
    // / `"kind":"contract"` since formula ASTs also contain `"kind"`
    // keys (e.g. forall, atomic) that would otherwise confuse a
    // naive scan.
    let is_implication = s.contains("\"kind\":\"implication\"");
    let is_contract = s.contains("\"kind\":\"contract\"");
    if is_implication {
        let antecedent_hash = json_string_field(s, "\"antecedentHash\":")
            .ok_or_else(|| BenchError::Lattice("missing antecedentHash".into()))?
            .to_string();
        let consequent_hash = json_string_field(s, "\"consequentHash\":")
            .ok_or_else(|| BenchError::Lattice("missing consequentHash".into()))?
            .to_string();
        let producer_id = json_string_field(s, "\"producedBy\":")
            .unwrap_or("")
            .to_string();
        let producer_signature = json_string_field(s, "\"producerSignature\":")
            .unwrap_or("")
            .to_string();
        let cid = json_string_field(s, "\"cid\":").unwrap_or("").to_string();
        Ok(ParseResult::Implication(ImpRecord {
            cid,
            antecedent_hash,
            consequent_hash,
            producer_id,
            producer_signature,
            envelope_bytes: inner.to_vec(),
        }))
    } else if is_contract {
        let pre_hash = json_string_field(s, "\"preHash\":")
            .unwrap_or("")
            .to_string();
        let post_hash = json_string_field(s, "\"postHash\":")
            .unwrap_or("")
            .to_string();
        if pre_hash.is_empty() || post_hash.is_empty() {
            Ok(ParseResult::Other)
        } else {
            Ok(ParseResult::Contract {
                pre_hash,
                post_hash,
            })
        }
    } else {
        Ok(ParseResult::Other)
    }
}

/// Tiny scanner: find the first JCS string field whose key prefix
/// matches `key_prefix`, return the value (slice between the next
/// pair of double quotes after the prefix).
fn json_string_field<'a>(s: &'a str, key_prefix: &str) -> Option<&'a str> {
    let i = s.find(key_prefix)?;
    let after = &s[i + key_prefix.len()..];
    let start_q = after.find('"')?;
    let rest = &after[start_q + 1..];
    let end_q = rest.find('"')?;
    Some(&rest[..end_q])
}

/// Pull the first member bytes out of a CBOR `{ ..., members: { <cid>:
/// <bstr>, ... }, ... }` envelope. We assume there is exactly one
/// member (the showcase fixture wraps each memento in a singleton
/// catalog) and that its value is a definite-length byte string.
///
/// This is a deliberate minimal CBOR parser: we walk the top-level
/// map, find the `members` key, and read its first map entry's value
/// bytes. Robust against the deterministic encoding the showcase
/// fixture produces.
fn extract_first_member(cbor: &[u8]) -> Option<&[u8]> {
    let mut p = Cursor { buf: cbor, pos: 0 };
    let n = p.read_map_head()?;
    for _ in 0..n {
        let key = p.read_tstr()?;
        if key == "members" {
            let m = p.read_map_head()?;
            if m == 0 {
                return None;
            }
            let _inner_key = p.read_tstr()?;
            return p.read_bstr_slice();
        } else {
            // Skip the value. This is a tagged-types-free encoding so
            // we can defer to a generic skipper.
            p.skip_item()?;
        }
    }
    None
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn read_byte(&mut self) -> Option<u8> {
        let b = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn read_n(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.pos + n > self.buf.len() {
            return None;
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }
    fn read_uint(&mut self, ai: u8) -> Option<u64> {
        match ai {
            0..=23 => Some(ai as u64),
            24 => Some(self.read_byte()? as u64),
            25 => {
                let s = self.read_n(2)?;
                Some(u16::from_be_bytes([s[0], s[1]]) as u64)
            }
            26 => {
                let s = self.read_n(4)?;
                Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as u64)
            }
            27 => {
                let s = self.read_n(8)?;
                Some(u64::from_be_bytes([
                    s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
                ]))
            }
            _ => None,
        }
    }
    fn read_map_head(&mut self) -> Option<u64> {
        let b = self.read_byte()?;
        let major = b >> 5;
        if major != 5 {
            return None;
        }
        self.read_uint(b & 0x1F)
    }
    #[allow(dead_code)] // symmetric to read_map_head; bench data only exercises maps but the API pair stays
    fn read_array_head(&mut self) -> Option<u64> {
        let b = self.read_byte()?;
        let major = b >> 5;
        if major != 4 {
            return None;
        }
        self.read_uint(b & 0x1F)
    }
    fn read_tstr(&mut self) -> Option<&'a str> {
        let b = self.read_byte()?;
        let major = b >> 5;
        if major != 3 {
            return None;
        }
        let n = self.read_uint(b & 0x1F)? as usize;
        let s = self.read_n(n)?;
        std::str::from_utf8(s).ok()
    }
    fn read_bstr_slice(&mut self) -> Option<&'a [u8]> {
        let b = self.read_byte()?;
        let major = b >> 5;
        if major != 2 {
            return None;
        }
        let n = self.read_uint(b & 0x1F)? as usize;
        self.read_n(n)
    }
    fn skip_item(&mut self) -> Option<()> {
        let b = self.read_byte()?;
        let major = b >> 5;
        let ai = b & 0x1F;
        match major {
            0 | 1 => {
                self.read_uint(ai)?;
                Some(())
            }
            2 | 3 => {
                let n = self.read_uint(ai)? as usize;
                self.read_n(n)?;
                Some(())
            }
            4 => {
                let n = self.read_uint(ai)?;
                for _ in 0..n {
                    self.skip_item()?;
                }
                Some(())
            }
            5 => {
                let n = self.read_uint(ai)?;
                for _ in 0..n {
                    self.skip_item()?;
                    self.skip_item()?;
                }
                Some(())
            }
            6 => {
                self.read_uint(ai)?;
                self.skip_item()
            }
            7 => {
                self.read_uint(ai)?;
                Some(())
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tier-2 verify helper
// ---------------------------------------------------------------------------

/// Recompute the JCS-unsigned bytes (envelope minus `cid` and
/// `producerSignature`) and verify the producer signature matches.
/// This is what a real verifier does on cache-hit. We don't go
/// through provekit-verifier here to avoid touching the parallel
/// agent's territory.
fn verify_envelope_signature(rec: &ImpRecord) -> bool {
    if rec.producer_id.is_empty() || rec.producer_signature.is_empty() {
        return false;
    }
    let s = match std::str::from_utf8(&rec.envelope_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };
    // Strip the cid + producerSignature key/value pairs from the JCS
    // canonical bytes by re-emitting an Object that excludes those
    // two fields. We parse the JSON minimally: the canonicalizer's
    // Value tree doesn't ship a parser, so we do a small
    // strip-and-re-emit instead.
    let stripped = strip_signed_fields(s);
    ed25519_verify_string(
        &rec.producer_id,
        &rec.producer_signature,
        stripped.as_bytes(),
    )
}

/// Remove the `"cid":"..."` and `"producerSignature":"..."` JSON
/// fields from an outer object's canonical JCS bytes. Only correct
/// for the specific JCS shape the kit produces (a flat outer object
/// where these two keys appear at the top level).
fn strip_signed_fields(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'{') {
        return s.to_string();
    }
    out.push('{');
    i += 1;
    let mut first = true;
    while i < bytes.len() {
        if bytes[i] == b'}' {
            out.push('}');
            break;
        }
        // skip leading comma
        if bytes[i] == b',' {
            i += 1;
            continue;
        }
        // Read key.
        if bytes[i] != b'"' {
            // unexpected; bail.
            return s.to_string();
        }
        let key_start = i + 1;
        let mut j = key_start;
        while j < bytes.len() && bytes[j] != b'"' {
            j += 1;
        }
        let key = &s[key_start..j];
        let key_end = j + 1;
        // Expect ':'
        if key_end >= bytes.len() || bytes[key_end] != b':' {
            return s.to_string();
        }
        // Read value: simple types only (string, integer, bool,
        // object, array). We need to skip to the matching boundary.
        let value_start = key_end + 1;
        let value_end = scan_value_end(bytes, value_start);
        let keep = key != "cid" && key != "producerSignature";
        if keep {
            if !first {
                out.push(',');
            }
            out.push('"');
            out.push_str(key);
            out.push('"');
            out.push(':');
            out.push_str(&s[value_start..value_end]);
            first = false;
        }
        i = value_end;
    }
    out
}

fn scan_value_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    if i >= bytes.len() {
        return i;
    }
    match bytes[i] {
        b'"' => {
            i += 1;
            while i < bytes.len() {
                let b = bytes[i];
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == b'"' {
                    return i + 1;
                }
                i += 1;
            }
            i
        }
        b'{' | b'[' => {
            let open = bytes[i];
            let close = if open == b'{' { b'}' } else { b']' };
            let mut depth = 1i32;
            i += 1;
            while i < bytes.len() && depth > 0 {
                let b = bytes[i];
                if b == b'"' {
                    // skip string
                    i += 1;
                    while i < bytes.len() {
                        let c = bytes[i];
                        if c == b'\\' {
                            i += 2;
                            continue;
                        }
                        if c == b'"' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    continue;
                }
                if b == open {
                    depth += 1;
                } else if b == close {
                    depth -= 1;
                }
                i += 1;
            }
            i
        }
        _ => {
            // bare scalar (number, true, false, null): runs until ',' or '}' or ']'
            while i < bytes.len() && bytes[i] != b',' && bytes[i] != b'}' && bytes[i] != b']' {
                i += 1;
            }
            i
        }
    }
}

// ---------------------------------------------------------------------------
// Tier-3 helpers
// ---------------------------------------------------------------------------

fn synth_smt(pre: &str, post: &str, salt: u64) -> String {
    // Build a small but non-trivial query whose shape mirrors what a
    // real handshake would emit: a forall-quantified Int problem
    // with arithmetic constraints derived from the two hash strings'
    // numeric content.
    let a = mix_hash_to_int(pre, salt) % 7919;
    let b = mix_hash_to_int(post, salt ^ 0xDEADBEEF) % 7919;
    format!(
        "(set-logic LIA)\n(declare-const x Int)\n(declare-const y Int)\n(assert (>= x {a}))\n(assert (<= y {b}))\n(assert (forall ((n Int)) (=> (and (>= n 0) (< n 1000)) (>= (+ x n) {a}))))\n(check-sat)\n(exit)\n",
        a = a,
        b = b,
    )
}

fn mix_hash_to_int(h: &str, salt: u64) -> u64 {
    let bytes = h.as_bytes();
    let mut acc: u64 = salt;
    for &b in bytes.iter().take(16) {
        acc = acc.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(b as u64);
    }
    acc
}

fn run_z3(z3_path: &str, smt: &str) -> Result<(), BenchError> {
    let mut child = Command::new(z3_path)
        .arg("-in")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| BenchError::Z3Spawn {
            path: z3_path.to_string(),
            source,
        })?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or(BenchError::Lattice("no stdin".into()))?;
        stdin.write_all(smt.as_bytes())?;
    }
    let _ = child.wait_with_output()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

/// Strip the "blake3-512:" prefix and decode the 128-hex digest into
/// 64 raw bytes. If the input is not the expected shape we return a
/// zeroed array (so the bench still measures a 64-byte memcmp).
fn digest_bytes(cid_or_hash: &str) -> [u8; 64] {
    let stripped = cid_or_hash
        .strip_prefix("blake3-512:")
        .unwrap_or(cid_or_hash);
    let mut out = [0u8; 64];
    if stripped.len() == 128 {
        if let Ok(bytes) = hex::decode(stripped) {
            if bytes.len() == 64 {
                out.copy_from_slice(&bytes);
            }
        }
    }
    out
}

/// Constant-time-ish 64-byte equality. Volatile-read the result so
/// the optimizer doesn't elide the comparison.
fn ct_eq_64(a: &[u8; 64], b: &[u8; 64]) -> bool {
    let mut diff: u8 = 0;
    for i in 0..64 {
        diff |= a[i] ^ b[i];
    }
    let v: u8 = unsafe { std::ptr::read_volatile(&diff) };
    v == 0
}

fn lcg_pick<'a, T>(state: &mut u64, items: &'a [T]) -> &'a T {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let i = (*state as usize) % items.len();
    &items[i]
}

// Suppress unused-import noise from the canonicalizer Value: the
// fixture module relies on it; the bench module only uses the
// hashing helper.
#[allow(dead_code)]
fn _value_keepalive() {
    let _v: Arc<Value> = Value::null();
}
