// Shim for <va/va.h> during bindgen: hwcontext_vaapi.h needs only these VA
// handle types (opaque handles / uint32 IDs), not the libva API.
#ifndef HWCONTEXT_STUB_VA_H
#define HWCONTEXT_STUB_VA_H

typedef void *VADisplay;
typedef unsigned int VASurfaceID;
typedef unsigned int VAConfigID;
typedef struct VASurfaceAttrib VASurfaceAttrib; /* opaque; by pointer */

#endif /* HWCONTEXT_STUB_VA_H */
