/* SPDX-License-Identifier: Apache-2.0 */
/* Two KUnit tests on checked(). One should compose SAT, one should
 * compose UNSAT against checked()'s post = result = x.
 */

struct kunit;
#define KUNIT_EXPECT_EQ(test, a, b) ((void)(a), (void)(b))
#define KUNIT_EXPECT_NE(test, a, b) ((void)(a), (void)(b))

extern int checked(int x);

void test_checked_returns_42(struct kunit *test) {
    int actual = checked(42);
    KUNIT_EXPECT_EQ(test, actual, 42);
}

void test_checked_does_not_return_42(struct kunit *test) {
    int actual = checked(42);
    KUNIT_EXPECT_NE(test, actual, 42);
}
