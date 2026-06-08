pub enum E {
    A(u32),
    B { x: u32 },
}

pub fn get_a_val(e: &E) -> u32 {
    match e {
        E::A(v) => *v,
        E::B { x } => *x,
    }
}
