/* @generated weak forward — getenv */
#include "os_shim.h"
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((weak)) char *os_shim_getenv(const char *a0) {
    return (char *)getenv(a0);
}
