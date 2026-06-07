#[provekit::boundary(concept = "concept:reverse-string", library = "rust-boundary-vendor", call = "reverse_chars")]
pub fn rev(s: &str) -> String {
    unimplemented!("materialize-fillable boundary")
}
