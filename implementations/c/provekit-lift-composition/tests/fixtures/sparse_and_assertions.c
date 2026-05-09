#define __user

int load_user_value(int __user *ptr)
{
    WARN_ON(!ptr);
    return ptr ? *ptr : -14;
}
