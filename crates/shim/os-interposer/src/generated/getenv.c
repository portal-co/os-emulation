/* @generated interposer bridge — getenv */
#include "os_shim.h"
#include <stdlib.h>

static char *__os_interpose_getenv(const char *name) {
    return os_shim_getenv(name);
}

#if defined(__APPLE__)
struct __interpose_tuple {
    const void *replacement;
    const void *replacee;
};
__attribute__((used)) static struct __interpose_tuple __os_interpose_getenv_tuple
    __attribute__((section("__DATA,__interpose"))) = {
    (const void *)&__os_interpose_getenv,
    (const void *)&getenv,
};
#endif

#if !defined(__APPLE__)
__attribute__((visibility("default"))) char *getenv(const char *name) {
    return __os_interpose_getenv(name);
}
#endif
