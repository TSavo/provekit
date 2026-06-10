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
    fn test_union_bits_ab_exact_row() {
        // Vendor source: bitflags 2.6.0 doc-example for union.
        // A | B has bits() == 1 | 2 == 3.
        assert_eq!((Flags::A | Flags::B).bits(), 3);
    }

    #[test]
    fn test_contains_a_in_ab_exact_row() {
        // Vendor source: bitflags 2.6.0 doc-example for contains.
        // A | B contains A.
        assert!((Flags::A | Flags::B).contains(Flags::A));
    }

    #[test]
    fn test_intersection_a_b_empty_exact_row() {
        // Vendor source: bitflags 2.6.0 doc-example for intersection.
        // A intersected with B has no bits in common.
        assert!(Flags::A.intersection(Flags::B).is_empty());
    }

    #[test]
    fn test_intersection_ab_a_bits_exact_row() {
        // Vendor source: bitflags 2.6.0 doc-example for intersection.
        // (A | B) intersected with A has bits() == 1.
        assert_eq!((Flags::A | Flags::B).intersection(Flags::A).bits(), 1);
    }

    #[test]
    fn test_empty_is_empty_exact_row() {
        // Vendor source: bitflags 2.6.0 doc-example for empty.
        // Flags::empty() has no bits set.
        assert!(Flags::empty().is_empty());
    }

    #[test]
    fn test_contains_b_in_abc_exact_row() {
        // Vendor source: bitflags 2.6.0 doc-example for contains.
        // A | B | C contains B.
        assert!((Flags::A | Flags::B | Flags::C).contains(Flags::B));
    }

    #[test]
    fn test_union_method_bits_ab_exact_row() {
        // Vendor source: bitflags 2.6.0 doc-example for union method.
        // A.union(B).bits() == 3.
        assert_eq!(Flags::A.union(Flags::B).bits(), 3);
    }

    #[test]
    fn test_all_contains_c_exact_row() {
        // Vendor source: bitflags 2.6.0 doc-example for all.
        // Flags::all() contains every defined flag, including C.
        assert!(Flags::all().contains(Flags::C));
    }

    #[test]
    fn test_abc_bits_exact_row() {
        // Vendor source: bitflags 2.6.0 bit value of A|B|C.
        // A=1, B=2, C=4: combined bits() == 7.
        assert_eq!((Flags::A | Flags::B | Flags::C).bits(), 7);
    }
}
