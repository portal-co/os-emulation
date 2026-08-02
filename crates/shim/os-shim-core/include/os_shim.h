#ifndef OS_SHIM_H
#define OS_SHIM_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

long os_shim_write(int fd, void *buf, long count);
void os_shim_exit(int code);
int os_shim_printf(const char *fmt, void *fn_ptr);
int os_shim_putchar(int c);
long os_shim_strlen(const char *s);
char *os_shim_getenv(const char *name);
int os_shim_execve(const char *path, char *const argv[], char *const envp[]);

#ifdef __cplusplus
}
#endif

#endif /* OS_SHIM_H */
