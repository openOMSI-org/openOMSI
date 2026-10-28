#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>


extern int _NSGetExecutablePath(char *buf, uint32_t *bufsize);
int main(int argc, char **argv) {
    (void)argc;
    uint32_t size = 4096;
    char *path = malloc(size);
    if (!path) return 1;
    if (_NSGetExecutablePath(path, &size) != 0) {
        char *larger = realloc(path, size);
        if (!larger) return 1;
        path = larger;
    }
    if (_NSGetExecutablePath(path, &size) != 0) {
        fputs("openOMSI: cannot locate its app bundle\n", stderr);
        return 1;
    }
    char *slash = strrchr(path, '/');
    if (!slash) {
        fputs("openOMSI: invalid app executable path\n", stderr);
        return 1;
    }
    *slash = '\0';
    const char *before = getenv("DYLD_LIBRARY_PATH");
    size_t env_size = strlen(path) + (before && *before ? strlen(before) + 2 : 1);
    char *libraries = malloc(env_size);
    if (!libraries) return 1;
    if (before && *before) snprintf(libraries, env_size, "%s:%s", path, before);
    else snprintf(libraries, env_size, "%s", path);
    if (setenv("DYLD_LIBRARY_PATH", libraries, 1) != 0 ||
        unsetenv("ANGLE_DEFAULT_PLATFORM") != 0) {
        perror("openOMSI: graphics environment");
        return 1;
    }
    size_t binary_size = strlen(path) + sizeof("/openomsi-bin");
    char *binary = malloc(binary_size);
    if (!binary) return 1;
    snprintf(binary, binary_size, "%s/openomsi-bin", path);
    argv[0] = binary;
    execv(binary, argv);
    perror("openOMSI: cannot start game");
    return 1;
}
