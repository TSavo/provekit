#define NULL ((void *)0)
#define ENOMEM 12

void BUG(void);
int WARN_ON(int cond);

#define BUG_ON(cond) do { if (cond) BUG(); } while (0)
#define BUILD_BUG_ON(cond) ((void)sizeof(char[1 - 2 * !!(cond)]))

void bug_on_requires_ptr(void *x)
{
    BUG_ON(x == NULL);
}

int warn_only(int cond)
{
    WARN_ON(cond);
    return cond;
}

int alloc_requires_buf(char *buf)
{
    if (!buf)
        return -ENOMEM;
    return 0;
}

int ret_must_be_nonnegative(int ret)
{
    if (ret < 0)
        return ret;
    return ret;
}
