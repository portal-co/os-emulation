/* @generated interposer bridge — printf */
#include "os_shim.h"
#include <dlfcn.h>
#include <stdio.h>

static int __os_load_printf(const char *fmt, ...) {
    (void)fmt;
    return os_shim_printf(fmt, 0);
}

#if defined(__APPLE__)
struct __interpose_tuple {
    const void *replacement;
    const void *replacee;
};
__attribute__((used)) static struct __interpose_tuple __os_interpose_printf_tuple
    __attribute__((section("__DATA,__interpose"))) = {
    (const void *)&__os_load_printf,
    (const void *)&printf,
};
#endif

#if !defined(__APPLE__)
__attribute__((visibility("default"))) int printf(const char *fmt, ...) {
    return __os_load_printf(fmt);
}
#endif
