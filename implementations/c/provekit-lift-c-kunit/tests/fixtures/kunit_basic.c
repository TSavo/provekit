struct kunit {};

#define KUNIT_CASE(fn) { fn }

static int foo(int x) {
    return x + 5;
}

static void pk_basic_test(struct kunit *test) {
    int x = 5;
    int r = foo(5);

    KUNIT_EXPECT_EQ(test, foo(5), 10);
    KUNIT_EXPECT_EQ(test, r, 10);
    KUNIT_EXPECT_EQ(test, x, 5);
}

struct kunit_case {
    void (*run_case)(struct kunit *test);
};

static struct kunit_case pk_cases[] = {
    KUNIT_CASE(pk_basic_test),
};
