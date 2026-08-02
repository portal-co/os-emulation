/* @generated weak forward — strlen */
#include "os_shim.h"
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((weak)) long os_shim_strlen(const char *a0) {
    return (long)strlen(a0);
}
