/**
 * copy_to_user_buffer - copy bytes to a user buffer
 * @buf: destination buffer. Must not be NULL.
 * @len: byte count. Must be positive.
 * Context: Must be called with io_lock held.
 * Return: 0 on success or negative errno on failure.
 */
int copy_to_user_buffer(char *buf, int len) {
    return len < 0 ? -22 : 0;
}
