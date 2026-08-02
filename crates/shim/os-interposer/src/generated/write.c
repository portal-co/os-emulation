/* @generated interposer bridge — write */
#include "os_shim.h"
#include <dlfcn.h>
#include <stddef.h>
#include <unistd.h>

static long (*__os_real_write)(int, void *, long);

static long __os_load_write(int fd, void *buf, long count) {
    if (!__os_real_write) {
        __os_real_write = (long (*)(int, void *, long))dlsym(RTLD_NEXT, "write");
    }
    return os_shim_write(fd, buf, count);
}

#if defined(__APPLE__)
struct __interpose_tuple {
    const void *replacement;
    const void *replacee;
};
__attribute__((used)) static struct __interpose_tuple __os_interpose_write_tuple
    __attribute__((section("__DATA,__interpose"))) = {
    (const void *)&__os_load_write,
    (const void *)&write,
};
#endif

#if !defined(__APPLE__)
__attribute__((visibility("default"))) long write(int fd, void *buf, size_t count) {
    return __os_load_write(fd, buf, (long)count);
}
#endif
