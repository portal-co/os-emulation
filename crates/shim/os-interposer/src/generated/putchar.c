/* @generated interposer bridge — putchar */
#include "os_shim.h"
#include <stdio.h>

static int __os_interpose_putchar(int c) {
    return os_shim_putchar(c);
}

#if defined(__APPLE__)
struct __interpose_tuple {
    const void *replacement;
    const void *replacee;
};
__attribute__((used)) static struct __interpose_tuple __os_interpose_putchar_tuple
    __attribute__((section("__DATA,__interpose"))) = {
    (const void *)&__os_interpose_putchar,
    (const void *)&putchar,
};
#endif

#if !defined(__APPLE__)
__attribute__((visibility("default"))) int putchar(int c) {
    return __os_interpose_putchar(c);
}
#endif
