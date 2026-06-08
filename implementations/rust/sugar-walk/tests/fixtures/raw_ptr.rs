// Fixture: function accepting a raw mutable pointer formal.
// Charon emits the formal's Ty as {"Untagged": {"RawPtr": [<inner_ty>, "Mut"]}}.
// The lifter must emit Effect::RawPointerProvenance { target: "p", mutable: true }.
unsafe fn write_via_raw(p: *mut u32) {
    unsafe { *p = 42; }
}
