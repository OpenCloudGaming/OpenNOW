// Shim for <mfxvideo.h> during bindgen: hwcontext_qsv.h needs only these QSV
// handle types (session handle; frame structs only by pointer), not the SDK.
#ifndef HWCONTEXT_STUB_MFXVIDEO_H
#define HWCONTEXT_STUB_MFXVIDEO_H

typedef struct _mfxSession *mfxSession;
typedef struct mfxFrameSurface1 mfxFrameSurface1; /* opaque; by pointer */
typedef struct mfxFrameInfo mfxFrameInfo;         /* opaque; by pointer */

#endif /* HWCONTEXT_STUB_MFXVIDEO_H */
