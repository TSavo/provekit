trait T {
    fn m(&self) -> u32;
}

struct S;

impl T for S {
    fn m(&self) -> u32 {
        42
    }
}

fn f(s: &S) -> u32 {
    s.m()
}
