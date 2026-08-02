/* @generated interposer bridge — exit */
#include "os_shim.h"
#include <dlfcn.h>
#include <stddef.h>
#include <unistd.h>

static void (*__os_real__exit)(int);

static void __os_load_exit(int code) {
    os_shim_exit(code);
}

#if defined(__APPLE__)
struct __interpose_tuple {
    const void *replacement;
    const void *replacee;
};
__attribute__((used)) static struct __interpose_tuple __os_interpose_exit_tuple
    __attribute__((section("__DATA,__interpose"))) = {
    (const void *)&__os_load_exit,
    (const void *)&_exit,
};
#endif

#if !defined(__APPLE__)
__attribute__((visibility("default"))) void _exit(int code) {
    __os_load_exit(code);
}
#endif
