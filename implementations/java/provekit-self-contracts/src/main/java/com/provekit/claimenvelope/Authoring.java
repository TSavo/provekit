// SPDX-License-Identifier: Apache-2.0
//
// Authoring tag — typed union mirrored from the rust/csharp kits.
// Lifted to a sealed-interface hierarchy.

package com.provekit.claimenvelope;

import com.provekit.canonicalizer.Value;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

public sealed interface Authoring permits Authoring.KitAuthor, Authoring.Lift, Authoring.Llm {

    /** Encode this authoring tag as a {@link Value} for JCS canonicalization. */
    Value toValue();

    /** {@code producerKind = "kit-author"} — the kit itself is the producer. */
    record KitAuthor(String author, String note) implements Authoring {

        public KitAuthor(String author) { this(author, null); }

        @Override
        public Value toValue() {
            List<Map.Entry<String, Value>> entries = new ArrayList<>();
            entries.add(Map.entry("producerKind", Value.ofString("kit-author")));
            entries.add(Map.entry("author", Value.ofString(author)));
            if (note != null && !note.isEmpty()) {
                entries.add(Map.entry("note", Value.ofString(note)));
            }
            return Value.ofObject(entries);
        }
    }

    /** {@code producerKind = "lift"} — produced by a lifter (e.g. ir-document). */
    record Lift(String lifter, String evidence, String sourceCid) implements Authoring {

        public Lift(String lifter, String evidence) { this(lifter, evidence, null); }

        @Override
        public Value toValue() {
            List<Map.Entry<String, Value>> entries = new ArrayList<>();
            entries.add(Map.entry("producerKind", Value.ofString("lift")));
            entries.add(Map.entry("lifter", Value.ofString(lifter)));
            entries.add(Map.entry("evidence", Value.ofString(evidence)));
            if (sourceCid != null && !sourceCid.isEmpty()) {
                entries.add(Map.entry("sourceCid", Value.ofString(sourceCid)));
            }
            return Value.ofObject(entries);
        }
    }

    /** {@code producerKind = "llm"} — produced by a language-model agent. */
    record Llm(String model, String version, String promptCid, double confidence,
               String rationale) implements Authoring {

        public Llm(String model, String version, String promptCid, double confidence) {
            this(model, version, promptCid, confidence, null);
        }

        @Override
        public Value toValue() {
            List<Map.Entry<String, Value>> entries = new ArrayList<>();
            entries.add(Map.entry("producerKind", Value.ofString("llm")));
            entries.add(Map.entry("llm", Value.ofString(model)));
            entries.add(Map.entry("llmVersion", Value.ofString(version)));
            entries.add(Map.entry("promptCid", Value.ofString(promptCid)));
            // confidence carried as a milli-int (matches the rust peer's
            // (confidence * 1000) integer encoding; JCS values are
            // integer-only on the number side).
            entries.add(Map.entry("confidence", Value.ofInt((long) (confidence * 1000.0))));
            if (rationale != null && !rationale.isEmpty()) {
                entries.add(Map.entry("rationale", Value.ofString(rationale)));
            }
            return Value.ofObject(entries);
        }
    }
}
