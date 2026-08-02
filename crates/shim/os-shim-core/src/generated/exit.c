/* @generated weak forward — exit */
#include "os_shim.h"
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((weak)) void os_shim_exit(int a0) {
    _exit(a0);
}
