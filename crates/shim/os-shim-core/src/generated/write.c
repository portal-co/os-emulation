/* @generated weak forward — write */
#include "os_shim.h"
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((weak)) long os_shim_write(int a0, void *a1, long a2) {
    return (long)write(a0, a1, (size_t)a2);
}
