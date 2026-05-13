/* SPDX-License-Identifier: Apache-2.0 */

#define BUG_ON(x) do { if (x) return -1; } while (0)
#define ENOMEM 12
#define EINVAL 22
#define __user
#define __rcu
#define __must_hold(x)
#define __acquires(x)
#define __releases(x)
#define assert(x) do { } while (0)
typedef unsigned long size_t;
typedef unsigned int gfp_t;
struct scatterlist { int length; };
struct sg_mapping_iter { int consumed; };
struct skcipher_request { int cryptlen; };
struct sk_buff { int len; };

int bug_on_nonnegative(int x) {
    BUG_ON(x < 0);
    return x;
}

int errno_guard(char *ptr) {
    if (!ptr) return -ENOMEM;
    return 0;
}

int user_buffer(__user char *buf) {
    return 0;
}

int held_lock(int x) __must_hold(lock) {
    return x;
}

int trailing_return(int x) {
    return x + 1;
}

int ret_guard(int ret) {
    if (ret < 0) return ret;
    return ret;
}

int goto_error(int x) {
    if (x == 0) goto error;
    return x;
error:
    return -1;
}

int assert_positive(int x) {
    assert(x > 0);
    return x;
}

int rcu_pointer(__rcu int *p) {
    return 0;
}

int sized_count(size_t n) {
    return 0;
}

int gfp_flags(gfp_t gfp) {
    return 0;
}

int acquire_lock(int x) __acquires(lock) {
    return x;
}

int release_lock(int x) __releases(lock) {
    return x;
}

int handled_null_store(int *p) {
    if (!p) return -EINVAL;
    *p = 0;
    return 0;
}

int sg_nents_for_len(struct scatterlist *sg, unsigned int len) {
    if (!sg) return -EINVAL;
    return sg->length + (int)len;
}

int sg_miter_next(struct sg_mapping_iter *miter) {
    if (!miter) return 0;
    return miter->consumed;
}

int crypto_skcipher_encrypt(struct skcipher_request *req) {
    if (!req) return -EINVAL;
    return req->cryptlen;
}

int crypto_skcipher_decrypt(struct skcipher_request *req) {
    if (!req) return -EINVAL;
    return req->cryptlen;
}

int skb_to_sgvec(struct sk_buff *skb, struct scatterlist *sg, int offset, int len) {
    if (!skb) return -EINVAL;
    if (!sg) return -EINVAL;
    return skb->len + sg->length + offset + len;
}

int __skb_to_sgvec(struct sk_buff *skb, struct scatterlist *sg, int offset, int len) {
    if (!skb) return -EINVAL;
    if (!sg) return -EINVAL;
    return skb->len + sg->length + offset + len;
}
