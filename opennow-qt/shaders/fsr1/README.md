# AMD FidelityFX Super Resolution 1

`ffx_a.h` and `ffx_fsr1.h` are unmodified copies from
[GPUOpen-Effects/FidelityFX-FSR](https://github.com/GPUOpen-Effects/FidelityFX-FSR),
commit `a21ffb8f6c13233ba336352bdff293894c706575`, directory `ffx-fsr/`.
Their original AMD copyright and MIT license notices are retained in each file.

OpenNOW uses the official full-precision EASU and RCAS implementations in QRhi
fragment passes. The adapters live in `../fsr_easu.frag` and `../fsr_rcas.frag`.
EASU consumes the entire SDR input texture. RCAS uses clamped edge fetches and
no color-space conversion or denoising. Sharpness zero skips RCAS; values 1–15
map linearly from 2 to 0 stops of sharpening attenuation. The final existing
video material continues to own output color conversion and composition.
