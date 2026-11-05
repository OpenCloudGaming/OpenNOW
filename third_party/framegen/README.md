# Framegen design attribution

The interpolation design, pacing rules and capability test credit
[Framegen](https://github.com/MONZikWasTaken/Framegen) by MONZik. The upstream
runtime attribution is MIT; its full notice remains in
[THIRD_PARTY_NOTICES](../../THIRD_PARTY_NOTICES).

The Windows client presents frames from the [FlyHigh engine](https://github.com/getflyhigh/flyhigh-engine).
The QRhi block-matching interpolator, identified as `motion`, remains for tests
and for builds that do not load FlyHigh. Neither path contains Framegen's
v7-small neural runtime or model weights. The settings link opens the FlyHigh
engine repository.

[WEIGHTS_LICENSE.md](./WEIGHTS_LICENSE.md) records the upstream model's separate
terms for reference only. No model weights are redistributed in this tree.
