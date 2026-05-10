int branch_foo(int x) {
    if (x == 0) return -22;
    return x;
}

int branch_else(int x) {
    if (x == 0) {
        return -22;
    } else {
        return x;
    }
}

int branch_block(int x) {
    if (x < 0) {
        int ignored = x;
        return -1;
    }
    return x + 1;
}

int branch_nested(int x) {
    if (x < 0) {
        if (x < -10) return -10;
        return -1;
    }
    return x;
}

int early_return(int err, int ok) {
    if (err) return -5;
    return ok;
}

int loop_refusal(int x) {
    while (x > 0) {
        x = x - 1;
    }
    return x;
}
