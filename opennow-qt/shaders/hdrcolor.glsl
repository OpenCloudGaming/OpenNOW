vec3 sdrToLinear(vec3 value)
{
    return mix(value / 12.92, pow((value + 0.055) / 1.055, vec3(2.4)),
               greaterThan(value, vec3(0.04045)));
}

vec3 linearToSdr(vec3 value)
{
    value = max(value, vec3(0.0));
    return mix(value * 12.92, 1.055 * pow(value, vec3(1.0 / 2.4)) - 0.055,
               greaterThan(value, vec3(0.0031308)));
}

vec3 pqToNits(vec3 value)
{
    vec3 p = pow(clamp(value, 0.0, 1.0), vec3(1.0 / 78.84375));
    return 10000.0 * pow(max(p - 0.8359375, 0.0)
        / max(18.8515625 - 18.6875 * p, 0.000001), vec3(1.0 / 0.1593017578125));
}

vec3 nitsToPq(vec3 value)
{
    vec3 p = pow(clamp(value / 10000.0, 0.0, 1.0), vec3(0.1593017578125));
    return pow((0.8359375 + 18.8515625 * p) / (1.0 + 18.6875 * p), vec3(78.84375));
}

vec3 hlgToNits(vec3 value)
{
    vec3 scene = mix(value * value / 3.0,
        (exp((value - 0.55991073) / 0.17883277) + 0.28466892) / 12.0,
        greaterThan(value, vec3(0.5)));
    float luminance = dot(scene, vec3(0.2627, 0.6780, 0.0593));
    return 1000.0 * scene * pow(max(luminance, 0.0), 0.2);
}

vec3 bt2020To709(vec3 value)
{
    return mat3(1.660491, -0.124550, -0.018151,
                -0.587641, 1.132900, -0.100579,
                -0.072850, -0.008349, 1.118730) * value;
}

vec3 bt709To2020(vec3 value)
{
    return mat3(0.627404, 0.069097, 0.016391,
                0.329283, 0.919540, 0.088013,
                0.043313, 0.011362, 0.895595) * value;
}

vec3 toneMapToSdr(vec3 nits, float whiteNits)
{
    float luminance = max(dot(nits, vec3(0.2126, 0.7152, 0.0722)), 0.0);
    vec3 mapped = nits / (whiteNits + luminance);
    float gray = luminance / (whiteNits + luminance);
    float low = min(min(mapped.r, mapped.g), mapped.b);
    float high = max(max(mapped.r, mapped.g), mapped.b);
    float saturation = 1.0;
    if (low < 0.0) saturation = min(saturation, gray / max(gray - low, 0.000001));
    if (high > 1.0) saturation = min(saturation, (1.0 - gray) / max(high - gray, 0.000001));
    return mix(vec3(gray), mapped, saturation);
}

vec3 outputColor(vec3 nits709, float outputMode, float whiteNits)
{
    if (outputMode > 2.5) return nits709 / whiteNits;
    if (outputMode > 1.5) return nitsToPq(bt709To2020(nits709));
    return nits709 / 80.0;
}

vec3 videoColor(vec3 encoded, float sourceSpace, float outputMode, float whiteNits, float hdrSupported)
{
    if (sourceSpace < 0.5) {
        if (outputMode < 0.5) return encoded;
        return outputColor(sdrToLinear(encoded) * whiteNits, outputMode, whiteNits);
    }
    vec3 nits = bt2020To709(sourceSpace < 1.5 ? pqToNits(encoded) : hlgToNits(encoded));
    if (outputMode < 0.5 || hdrSupported < 0.5) {
        vec3 sdr = toneMapToSdr(nits, whiteNits);
        return outputMode < 0.5 ? linearToSdr(sdr) : outputColor(sdr * whiteNits, outputMode, whiteNits);
    }
    return outputColor(nits, outputMode, whiteNits);
}
