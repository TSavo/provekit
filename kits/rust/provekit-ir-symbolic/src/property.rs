//! Property and bridge collectors. Mirrors `src/ir/symbolic/property.ts`.
//!
//! Thread-local active collector. `begin_collecting()` opens it; `must()`,
//! `property()`, and `bridge()` push declarations; the returned
//! `FinishHandle::finish()` closes the collector and returns the captured
//! list. The collector is non-reentrant: opening one while another is
//! active is an error.

use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::types::IrFormula;

// ---------------------------------------------------------------------------
// Declaration types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Declaration {
    #[serde(rename = "property")]
    Property {
        name: String,
        formula: IrFormula,
    },
    #[serde(rename = "bridge")]
    Bridge {
        name: String,
        #[serde(rename = "sourceSymbol")]
        source_symbol: String,
        #[serde(rename = "sourceLayer")]
        source_layer: String,
        #[serde(rename = "targetContractCid")]
        target_contract_cid: String,
        #[serde(rename = "targetLayer")]
        target_layer: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    },
}

impl Declaration {
    pub fn name(&self) -> &str {
        match self {
            Declaration::Property { name, .. } | Declaration::Bridge { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BridgeSpec {
    pub source_symbol: String,
    pub source_layer: String,
    pub target_contract_cid: String,
    pub target_layer: String,
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// Collector state — thread-local
// ---------------------------------------------------------------------------

thread_local! {
    static ACTIVE: RefCell<Option<Vec<Declaration>>> = const { RefCell::new(None) };
    static DESCRIBE_PATH: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Handle returned by `begin_collecting`. Call `.finish()` to retrieve the
/// captured declarations and clear the active collector.
#[must_use = "the collector remains open until finish() is called"]
pub struct FinishHandle {
    _private: (),
}

impl FinishHandle {
    /// Close the collector and return the captured declarations.
    pub fn finish(self) -> Vec<Declaration> {
        ACTIVE.with(|cell| {
            cell.borrow_mut().take().unwrap_or_default()
        })
    }
}

/// Begin collecting declarations. Returns a handle whose `.finish()` ends
/// collection. Non-reentrant: panics if a collection is already active.
pub fn begin_collecting() -> FinishHandle {
    ACTIVE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_some() {
            panic!(
                "begin_collecting: another collection is already active; lifting is not re-entrant"
            );
        }
        *slot = Some(Vec::new());
    });
    FinishHandle { _private: () }
}

/// Test helper: clear any active collector and reset describe path +
/// quantifier counter. Use only in test setup/teardown.
pub fn _reset_collector() {
    ACTIVE.with(|cell| { *cell.borrow_mut() = None; });
    DESCRIBE_PATH.with(|cell| { cell.borrow_mut().clear(); });
    crate::quantifiers::_reset_counter();
}

// ---------------------------------------------------------------------------
// describe / must / property / bridge
// ---------------------------------------------------------------------------

/// Open a named describe block. Body runs immediately; nested `must()`
/// calls register with the describe path prepended (joined by " > ").
pub fn describe<F: FnOnce()>(name: &str, body: F) {
    DESCRIBE_PATH.with(|cell| cell.borrow_mut().push(name.to_string()));
    // Use catch_unwind-equivalent? No — keep it simple: panic propagates,
    // and we rely on the caller resetting via _reset_collector if needed.
    // We DO want to pop the path even on panic; use a guard.
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            DESCRIBE_PATH.with(|cell| {
                cell.borrow_mut().pop();
            });
        }
    }
    let _g = Guard;
    body();
}

/// describe.skip equivalent — body never runs.
pub fn describe_skip<F: FnOnce()>(_name: &str, _body: F) {
    // intentionally drops the body without invoking it
}

fn current_full_name(name: &str) -> String {
    DESCRIBE_PATH.with(|cell| {
        let path = cell.borrow();
        if path.is_empty() {
            name.to_string()
        } else {
            format!("{} > {}", path.join(" > "), name)
        }
    })
}

/// Declare a named obligation. The active describe path prefixes the name.
pub fn must(name: &str, formula: IrFormula) {
    let full = current_full_name(name);
    property(&full, formula);
}

/// Skip an obligation (no-op).
pub fn must_skip(_name: &str, _formula: IrFormula) {}

/// Declare a property directly (no describe-path prefix applied).
pub fn property(name: &str, formula: IrFormula) {
    ACTIVE.with(|cell| {
        let mut slot = cell.borrow_mut();
        match slot.as_mut() {
            Some(decls) => decls.push(Declaration::Property {
                name: name.to_string(),
                formula,
            }),
            None => panic!(
                "property(\"{}\", ...) called outside an active collector. \
                 Call begin_collecting() first.",
                name
            ),
        }
    });
}

/// Declare a bridge from a host-language symbol to a deeper-layer contract.
pub fn bridge(name: &str, spec: BridgeSpec) {
    ACTIVE.with(|cell| {
        let mut slot = cell.borrow_mut();
        match slot.as_mut() {
            Some(decls) => decls.push(Declaration::Bridge {
                name: name.to_string(),
                source_symbol: spec.source_symbol,
                source_layer: spec.source_layer,
                target_contract_cid: spec.target_contract_cid,
                target_layer: spec.target_layer,
                notes: spec.notes,
            }),
            None => panic!(
                "bridge(\"{}\", ...) called outside an active collector.",
                name
            ),
        }
    });
}

// ---------------------------------------------------------------------------
// Macro sugar
// ---------------------------------------------------------------------------

/// `must!("name", formula)` — sweetened call to `must`.
#[macro_export]
macro_rules! must {
    ($name:expr, $formula:expr) => {
        $crate::property::must($name, $formula)
    };
}

/// `describe!("name", { body })` — sweetened call to `describe`.
/// The body block runs in an `FnOnce` closure.
#[macro_export]
macro_rules! describe {
    ($name:expr, $body:block) => {
        $crate::property::describe($name, || $body)
    };
}

/// `bridge!("name", BridgeSpec { ... })` — sweetened call to `bridge`.
#[macro_export]
macro_rules! bridge {
    ($name:expr, $spec:expr) => {
        $crate::property::bridge($name, $spec)
    };
}
