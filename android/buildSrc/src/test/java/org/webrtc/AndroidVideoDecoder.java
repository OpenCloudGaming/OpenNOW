package org.webrtc;

/** Models the calls made for every hardware-decoded texture output. */
public class AndroidVideoDecoder {
    public boolean textureMode = true;
    public boolean failHelper;
    public int codecInitializations;
    public int codecReleases;
    private boolean running;
    private VideoDecoder.Callback callback;
    private SurfaceTextureHelper surfaceTextureHelper;

    public VideoCodecStatus initDecode(VideoDecoder.Settings settings, VideoDecoder.Callback callback) {
        this.callback = callback;
        if (textureMode) {
            surfaceTextureHelper = createSurfaceTextureHelper();
            surfaceTextureHelper.startListening(this);
        }
        return initDecodeInternal(settings.width, settings.height);
    }

    private SurfaceTextureHelper createSurfaceTextureHelper() {
        return failHelper ? null : new SurfaceTextureHelper();
    }

    private VideoCodecStatus initDecodeInternal(int width, int height) {
        codecInitializations++;
        running = true;
        return VideoCodecStatus.OK;
    }

    public VideoCodecStatus release() {
        if (running) {
            running = false;
            codecReleases++;
        }
        if (surfaceTextureHelper != null) {
            surfaceTextureHelper.stopListening();
            surfaceTextureHelper.dispose();
            surfaceTextureHelper = null;
        }
        callback = null;
        return VideoCodecStatus.OK;
    }

    public VideoDecoder.Callback getCallback() { return callback; }
    public SurfaceTextureHelper getHelper() { return surfaceTextureHelper; }

    public void deliverTextureFrame(SurfaceTextureHelper helper, int width, int height, int rotation) {
        helper.setTextureSize(width, height);
        helper.setFrameRotation(rotation);
    }
}
