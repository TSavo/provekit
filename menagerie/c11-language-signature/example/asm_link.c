int asm_link(int x) {
    __asm__ volatile("nop");
    return x;
}
