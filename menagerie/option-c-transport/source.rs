// Exhibit source: a small Rust function using Option<T>
// Used as the source-side term for the M+N transport demonstration.
fn maybe_double(x: Option<i32>) -> Option<i32> {
    x.map(|n| n * 2)
}
