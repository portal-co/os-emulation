/* @generated weak forward — printf */
#include "os_shim.h"
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((weak)) int os_shim_printf(const char *a0, void *a1) {
    return (int)printf(a0, a1);
}
