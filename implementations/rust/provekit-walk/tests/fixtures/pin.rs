// Fixture: function accepting a Pin<&mut u32> formal.
// Charon emits the formal's Ty as an Adt referencing core::pin::Pin.
// The lifter must emit Effect::PinnedReference { target: "pinned" }.
use std::pin::Pin;

fn take_pin(pinned: Pin<&mut u32>) {
    let _ = pinned;
}
