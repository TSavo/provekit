use bitflags::bitflags;

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Flags: u32 {
        const A = 1;
        const B = 2;
        const C = 4;
    }
}

#[cfg(test)]
mod tests {
    use super::Flags;

    #[test]
    fn test_union_bits_ab_contradiction() {
        // Negative control derived from bitflags 2.6.0 doc-example for union.
        // The real result is 3 but we also assert 7, which is a contradiction.
        assert_eq!((Flags::A | Flags::B).bits(), 3);
        assert_eq!((Flags::A | Flags::B).bits(), 7);
    }
}
