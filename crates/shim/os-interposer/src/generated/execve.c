/* @generated interposer bridge — execve */
#include "os_shim.h"
#include <unistd.h>

static int __os_interpose_execve(const char *path, char *const argv[], char *const envp[]) {
    return os_shim_execve(path, argv, envp);
}

#if defined(__APPLE__)
struct __interpose_tuple {
    const void *replacement;
    const void *replacee;
};
__attribute__((used)) static struct __interpose_tuple __os_interpose_execve_tuple
    __attribute__((section("__DATA,__interpose"))) = {
    (const void *)&__os_interpose_execve,
    (const void *)&execve,
};
#endif

#if !defined(__APPLE__)
__attribute__((visibility("default"))) int execve(const char *path, char *const argv[], char *const envp[]) {
    return __os_interpose_execve(path, argv, envp);
}
#endif
