#include <va/va.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

VADisplay vaGetDisplayDRM(int fd) {
    char link[64], path[1024];
    snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
    ssize_t size = readlink(link, path, sizeof(path) - 1);
    if (size < 0) return NULL;
    path[size] = 0;
    char *node = strstr(path, "renderD");
    return node ? (void *)(uintptr_t)atoi(node + 7) : NULL;
}

VAStatus vaInitialize(VADisplay display, int *major, int *minor) {
    *major = 1;
    *minor = 20;
    return VA_STATUS_SUCCESS;
}

VAStatus vaTerminate(VADisplay display) { return VA_STATUS_SUCCESS; }
int vaMaxNumProfiles(VADisplay display) { return 1; }
int vaMaxNumEntrypoints(VADisplay display) { return 1; }

VAStatus vaQueryConfigProfiles(VADisplay display, VAProfile *profiles, int *count) {
    profiles[0] = getenv("MOCK_FIRST_UNSUPPORTED") && (uintptr_t)display == 128
        ? VAProfileMPEG2Main : VAProfileH264Main;
    *count = 1;
    return VA_STATUS_SUCCESS;
}

VAStatus vaQueryConfigEntrypoints(VADisplay display, VAProfile profile, VAEntrypoint *entries, int *count) {
    entries[0] = VAEntrypointVLD;
    *count = 1;
    return VA_STATUS_SUCCESS;
}

VAStatus vaCreateConfig(VADisplay display, VAProfile profile, VAEntrypoint entry,
                        VAConfigAttrib *attributes, int count, VAConfigID *id) {
    *id = 1;
    return VA_STATUS_SUCCESS;
}

VAStatus vaDestroyConfig(VADisplay display, VAConfigID id) { return VA_STATUS_SUCCESS; }

VAStatus vaCreateContext(VADisplay display, VAConfigID config, int width, int height,
                         int flags, VASurfaceID *surfaces, int count, VAContextID *id) {
    fprintf(stderr, "mock context node=%lu width=%d height=%d targets=%d\n",
            (unsigned long)(uintptr_t)display, width, height, count);
    if (width == 16 && height == 16 && (getenv("MOCK_REJECT_TINY")
            || (getenv("MOCK_FIRST_TINY") && (uintptr_t)display == 128))) {
        return VA_STATUS_ERROR_RESOLUTION_NOT_SUPPORTED;
    }
    *id = 1;
    return VA_STATUS_SUCCESS;
}

VAStatus vaDestroyContext(VADisplay display, VAContextID id) { return VA_STATUS_SUCCESS; }

VAStatus vaCreateSurfaces(VADisplay display, unsigned int format, unsigned int width,
                          unsigned int height, VASurfaceID *ids, unsigned int count,
                          VASurfaceAttrib *attributes, unsigned int attribute_count) {
    for (unsigned int index = 0; index < count; index++) ids[index] = index + 1;
    return VA_STATUS_SUCCESS;
}

VAStatus vaDestroySurfaces(VADisplay display, VASurfaceID *ids, int count) { return VA_STATUS_SUCCESS; }
const char *vaQueryVendorString(VADisplay display) { return "OpenNOW mock VA-API"; }
const char *vaErrorStr(VAStatus status) {
    return status == VA_STATUS_ERROR_RESOLUTION_NOT_SUPPORTED ? "resolution not supported" : "mock error";
}
