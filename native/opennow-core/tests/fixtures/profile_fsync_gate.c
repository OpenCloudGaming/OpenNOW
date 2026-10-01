#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <sys/uio.h>
#include <unistd.h>

static void report_main_error(int fd, const void *data, size_t length)
{
    static int reported;
    if (fd != STDERR_FILENO || !memmem(data, length, "opennow-core:", 13)) return;
    const char *signal_fd = getenv("PROFILE_ERROR_FD");
    if (!signal_fd || __atomic_exchange_n(&reported, 1, __ATOMIC_SEQ_CST)) return;
    ssize_t (*real_write)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    const char signal = 'e';
    real_write(atoi(signal_fd), &signal, 1);
}

ssize_t write(int fd, const void *data, size_t length)
{
    ssize_t (*real_write)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    report_main_error(fd, data, length);
    return real_write(fd, data, length);
}

ssize_t writev(int fd, const struct iovec *vectors, int count)
{
    ssize_t (*real_writev)(int, const struct iovec *, int) = dlsym(RTLD_NEXT, "writev");
    for (int i = 0; i < count; ++i)
        report_main_error(fd, vectors[i].iov_base, vectors[i].iov_len);
    return real_writev(fd, vectors, count);
}

int fsync(int fd)
{
    static int entered;
    int (*real_fsync)(int) = dlsym(RTLD_NEXT, "fsync");
    int result = real_fsync(fd);
    int saved_errno = errno;
    const char *armed = getenv("PROFILE_FSYNC_ARMED");
    char link[64], path[4096];
    snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
    ssize_t length = readlink(link, path, sizeof(path) - 1);
    if (length > 0) path[length] = 0;
    if (armed && access(armed, F_OK) == 0 && length > 0
        && strstr(path, "/settings.json.tmp")
        && !__atomic_exchange_n(&entered, 1, __ATOMIC_SEQ_CST)) {
        const char *ready_fd = getenv("PROFILE_FSYNC_READY_FD");
        const char *release_fd = getenv("PROFILE_FSYNC_RELEASE_FD");
        if (ready_fd && release_fd) {
            const char signal = 's';
            write(atoi(ready_fd), &signal, 1);
            char release;
            while (read(atoi(release_fd), &release, 1) < 0 && errno == EINTR) {}
        }
    }
    errno = saved_errno;
    return result;
}
