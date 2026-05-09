struct kunit {};

#define KUNIT_CASE(fn) { fn }

static int add_one(int x) {
    return x + 1;
}

static void pk_basic_test(struct kunit *test) {
    int x = 5;

    KUNIT_EXPECT_EQ(test, x, 5);
    KUNIT_EXPECT_NE(test, add_one(1), 1);
    KUNIT_EXPECT_TRUE(test, x > 0);
    KUNIT_EXPECT_NOT_NULL(test, test);
}

struct kunit_case {
    void (*run_case)(struct kunit *test);
};

static struct kunit_case pk_cases[] = {
    KUNIT_CASE(pk_basic_test),
};
