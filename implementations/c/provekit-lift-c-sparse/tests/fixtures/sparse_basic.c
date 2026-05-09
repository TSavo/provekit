#define __user
#define __must_hold(x)

int copy_name(char __user *buf, int len)
{
    return len;
}

void update_locked(int *state) __must_hold(lock)
{
    *state = 1;
}
