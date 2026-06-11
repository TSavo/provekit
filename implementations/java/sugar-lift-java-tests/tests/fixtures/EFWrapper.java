// Effectively-final Voltron fixture: outer wrapper, private field, no final keyword.
// Both EFBox.value and EFWrapper.box are private without final — both proved by scan.
public final class EFWrapper {
    private EFBox box; // no final keyword — effectively final by scan
    EFWrapper(EFBox b) { this.box = b; }
    EFBox unwrap() { return this.box; }
}
