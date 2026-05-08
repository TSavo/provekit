void check_value(int value)
{
    WARN_ON(value < 0);
    BUILD_BUG_ON(sizeof(int) < 4);
}
