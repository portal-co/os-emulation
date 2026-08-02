/* @generated interposer bridge — strlen */
#include "os_shim.h"
#include <string.h>

static long __os_interpose_strlen(const char *s) {
    return os_shim_strlen(s);
}

#if defined(__APPLE__)
struct __interpose_tuple {
    const void *replacement;
    const void *replacee;
};
__attribute__((used)) static struct __interpose_tuple __os_interpose_strlen_tuple
    __attribute__((section("__DATA,__interpose"))) = {
    (const void *)&__os_interpose_strlen,
    (const void *)&strlen,
};
#endif

#if !defined(__APPLE__)
__attribute__((visibility("default"))) long strlen(const char *s) {
    return __os_interpose_strlen(s);
}
#endif
