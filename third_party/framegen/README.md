# Framegen third-party notice (FG-Legacy)

OpenNOW's local frame generation is built on [Framegen](https://github.com/MONZikWasTaken/Framegen),
the real-time neural frame interpolation project by MONZik. Inside OpenNOW the integration is
called **FG-Legacy**: it carries the Framegen v7-small (v7s) model into the native Qt/QRhi
renderer, and it is planned to be replaced later by the FlyHigh project (a new model and runtime
from the same author, built for the same slot).

The Frame generation settings row links to the upstream repository.

| Component | Origin | License |
| --- | --- | --- |
| Interpolator design, pacing rules and capability test | Framegen runtime (`framegen` npm: JS + WGSL), ported to QRhi | MIT |
| Model weights (`rt_v7s.*`, v7-small) | Framegen release | Non-commercial research / personal use only, see [WEIGHTS_LICENSE.md](./WEIGHTS_LICENSE.md) |

OpenNOW previously shipped Framegen in its Electron client (PR #718 / #763). The Qt rebuild
removed that integration together with these notices; FG-Legacy restores both.
