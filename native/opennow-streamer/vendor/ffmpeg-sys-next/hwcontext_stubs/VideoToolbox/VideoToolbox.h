// Shim for <VideoToolbox/VideoToolbox.h> during bindgen: AVVTFramesContext needs
// no VT types, but the header's function decls reference these CF/CV handles.
#ifndef HWCONTEXT_STUB_VIDEOTOOLBOX_H
#define HWCONTEXT_STUB_VIDEOTOOLBOX_H

#include <stdbool.h>

typedef const struct __CFString *CFStringRef;
typedef struct __CVBuffer *CVPixelBufferRef;

#endif /* HWCONTEXT_STUB_VIDEOTOOLBOX_H */
