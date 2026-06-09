/// Simple float fixture: takes an f64, returns an f64.
/// Used to verify that sugar-walk lifts f64 formals/return to the
/// platform-free Real sort at the LLBC layer.
fn scale(x: f64) -> f64 {
    x * 2.0
}
