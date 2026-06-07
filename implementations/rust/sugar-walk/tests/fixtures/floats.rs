/// Simple float fixture: takes an f64, returns an f64.
/// Used to verify that provekit-walk lifts f64 formals/return to
/// Sort::Float { width: 64 } at the LLBC layer.
fn scale(x: f64) -> f64 {
    x * 2.0
}
