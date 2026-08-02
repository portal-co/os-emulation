/* @generated weak forward — putchar */
#include "os_shim.h"
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((weak)) int os_shim_putchar(int a0) {
    return (int)putchar(a0);
}
