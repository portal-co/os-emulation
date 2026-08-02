/* @generated weak forward — execve */
#include "os_shim.h"
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((weak)) int os_shim_execve(const char *a0, char *const *a1, char *const *a2) {
    return (int)execve(a0, (char *const *)a1, (char *const *)a2);
}
