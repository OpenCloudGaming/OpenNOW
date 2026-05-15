#import "OPNUIHelpers.h"
#import "OPNColorTokens.h"
#import <AVFoundation/AVFoundation.h>
#include <cmath>

NSString *const OPNInterfacePreferencesDidChangeNotification = @"OpenNOW.InterfacePreferencesDidChange";

static NSString *const OPNAccentRedDefaultsKey = @"OpenNOW.Interface.AccentRed";
static NSString *const OPNAccentGreenDefaultsKey = @"OpenNOW.Interface.AccentGreen";
static NSString *const OPNAccentBlueDefaultsKey = @"OpenNOW.Interface.AccentBlue";
static NSString *const OPNPosterSizeScaleDefaultsKey = @"OpenNOW.Interface.PosterSizeScale";
static NSString *const OPNControllerGridItemScaleDefaultsKey = @"OpenNOW.Interface.ControllerGridItemScale";
static NSString *const OPNAutoFullScreenDefaultsKey = @"OpenNOW.Interface.AutoFullScreen";
static NSString *const OPNControllerModeDefaultsKey = @"OpenNOW.Interface.ControllerMode";
static NSString *const OPNBackgroundAnimationDefaultsKey = @"OpenNOW.Interface.BackgroundAnimation";
static NSString *const OPNDerivedAccentColorsDefaultsKey = @"OpenNOW.Interface.DerivedAccentColors";
static NSString *const OPNBackgroundTintStrengthDefaultsKey = @"OpenNOW.Interface.BackgroundTintStrength";
static NSString *const OPNControllerLibraryShortcutDefaultsKey = @"OpenNOW.Interface.ControllerLibraryShortcut";
static const CGFloat OPNMinimumPosterSizeScale = 0.80;
static const CGFloat OPNMaximumPosterSizeScale = 1.30;
static const CGFloat OPNMinimumControllerGridItemScale = 0.80;
static const CGFloat OPNMaximumControllerGridItemScale = 1.40;
static const unsigned OPNDefaultAccentRGB = 0x7CF1B1;
static const CGFloat OPNDefaultBackgroundTintStrength = 0.32;
static const uint16_t OPNDefaultControllerLibraryShortcutMask = 0x0010 | 0x0020;

static int OPNClampedColorByte(NSInteger value) {
    return (int)MAX(0, MIN(value, 255));
}

unsigned OpnBlendRGB(unsigned rgb, unsigned target, CGFloat amount) {
    amount = MAX(0.0, MIN(amount, 1.0));
    int r = (int)std::round(((rgb >> 16) & 0xFF) * (1.0 - amount) + ((target >> 16) & 0xFF) * amount);
    int g = (int)std::round(((rgb >> 8) & 0xFF) * (1.0 - amount) + ((target >> 8) & 0xFF) * amount);
    int b = (int)std::round((rgb & 0xFF) * (1.0 - amount) + (target & 0xFF) * amount);
    return ((unsigned)OPNClampedColorByte(r) << 16) | ((unsigned)OPNClampedColorByte(g) << 8) | (unsigned)OPNClampedColorByte(b);
}

unsigned OpnCurrentAccentRGB(void) {
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
    if (![defaults objectForKey:OPNAccentRedDefaultsKey] ||
        ![defaults objectForKey:OPNAccentGreenDefaultsKey] ||
        ![defaults objectForKey:OPNAccentBlueDefaultsKey]) {
        return OPNDefaultAccentRGB;
    }
    int r = OPNClampedColorByte([defaults integerForKey:OPNAccentRedDefaultsKey]);
    int g = OPNClampedColorByte([defaults integerForKey:OPNAccentGreenDefaultsKey]);
    int b = OPNClampedColorByte([defaults integerForKey:OPNAccentBlueDefaultsKey]);
    return ((unsigned)r << 16) | ((unsigned)g << 8) | (unsigned)b;
}

void OpnSetCurrentAccentRGB(unsigned rgb) {
    rgb &= 0xFFFFFF;
    if (rgb == OpnCurrentAccentRGB()) return;
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
    [defaults setInteger:(NSInteger)((rgb >> 16) & 0xFF) forKey:OPNAccentRedDefaultsKey];
    [defaults setInteger:(NSInteger)((rgb >> 8) & 0xFF) forKey:OPNAccentGreenDefaultsKey];
    [defaults setInteger:(NSInteger)(rgb & 0xFF) forKey:OPNAccentBlueDefaultsKey];
    [defaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

CGFloat OpnPosterSizeScale(void) {
    id stored = [NSUserDefaults.standardUserDefaults objectForKey:OPNPosterSizeScaleDefaultsKey];
    CGFloat scale = [stored respondsToSelector:@selector(doubleValue)] ? (CGFloat)[stored doubleValue] : 1.0;
    if (!std::isfinite(scale)) scale = 1.0;
    return MAX(OPNMinimumPosterSizeScale, MIN(scale, OPNMaximumPosterSizeScale));
}

void OpnSetPosterSizeScale(CGFloat scale) {
    if (!std::isfinite(scale)) scale = 1.0;
    CGFloat clampedScale = MAX(OPNMinimumPosterSizeScale, MIN(scale, OPNMaximumPosterSizeScale));
    if (std::fabs(clampedScale - OpnPosterSizeScale()) < 0.001) return;
    [NSUserDefaults.standardUserDefaults setDouble:clampedScale forKey:OPNPosterSizeScaleDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

CGFloat OpnControllerGridItemScale(void) {
    id stored = [NSUserDefaults.standardUserDefaults objectForKey:OPNControllerGridItemScaleDefaultsKey];
    CGFloat scale = [stored respondsToSelector:@selector(doubleValue)] ? (CGFloat)[stored doubleValue] : 1.0;
    if (!std::isfinite(scale)) scale = 1.0;
    return MAX(OPNMinimumControllerGridItemScale, MIN(scale, OPNMaximumControllerGridItemScale));
}

void OpnSetControllerGridItemScale(CGFloat scale) {
    if (!std::isfinite(scale)) scale = 1.0;
    CGFloat clampedScale = MAX(OPNMinimumControllerGridItemScale, MIN(scale, OPNMaximumControllerGridItemScale));
    if (std::fabs(clampedScale - OpnControllerGridItemScale()) < 0.001) return;
    [NSUserDefaults.standardUserDefaults setDouble:clampedScale forKey:OPNControllerGridItemScaleDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

BOOL OpnAutoFullScreenEnabled(void) {
    return [NSUserDefaults.standardUserDefaults boolForKey:OPNAutoFullScreenDefaultsKey];
}

void OpnSetAutoFullScreenEnabled(BOOL enabled) {
    if (enabled == OpnAutoFullScreenEnabled()) return;
    [NSUserDefaults.standardUserDefaults setBool:enabled forKey:OPNAutoFullScreenDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

BOOL OpnControllerModeEnabled(void) {
    return [NSUserDefaults.standardUserDefaults boolForKey:OPNControllerModeDefaultsKey];
}

void OpnSetControllerModeEnabled(BOOL enabled) {
    if (enabled == OpnControllerModeEnabled()) return;
    [NSUserDefaults.standardUserDefaults setBool:enabled forKey:OPNControllerModeDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

BOOL OpnBackgroundAnimationEnabled(void) {
    id stored = [NSUserDefaults.standardUserDefaults objectForKey:OPNBackgroundAnimationDefaultsKey];
    return stored ? [NSUserDefaults.standardUserDefaults boolForKey:OPNBackgroundAnimationDefaultsKey] : YES;
}

void OpnSetBackgroundAnimationEnabled(BOOL enabled) {
    if (enabled == OpnBackgroundAnimationEnabled()) return;
    [NSUserDefaults.standardUserDefaults setBool:enabled forKey:OPNBackgroundAnimationDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

BOOL OpnDerivedAccentColorsEnabled(void) {
    id stored = [NSUserDefaults.standardUserDefaults objectForKey:OPNDerivedAccentColorsDefaultsKey];
    return stored ? [NSUserDefaults.standardUserDefaults boolForKey:OPNDerivedAccentColorsDefaultsKey] : YES;
}

void OpnSetDerivedAccentColorsEnabled(BOOL enabled) {
    if (enabled == OpnDerivedAccentColorsEnabled()) return;
    [NSUserDefaults.standardUserDefaults setBool:enabled forKey:OPNDerivedAccentColorsDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

CGFloat OpnBackgroundTintStrength(void) {
    id stored = [NSUserDefaults.standardUserDefaults objectForKey:OPNBackgroundTintStrengthDefaultsKey];
    CGFloat strength = [stored respondsToSelector:@selector(doubleValue)] ? (CGFloat)[stored doubleValue] : OPNDefaultBackgroundTintStrength;
    if (!std::isfinite(strength)) strength = OPNDefaultBackgroundTintStrength;
    return MAX(0.0, MIN(strength, 1.0));
}

void OpnSetBackgroundTintStrength(CGFloat strength) {
    if (!std::isfinite(strength)) strength = OPNDefaultBackgroundTintStrength;
    CGFloat clampedStrength = MAX(0.0, MIN(strength, 1.0));
    if (std::fabs(clampedStrength - OpnBackgroundTintStrength()) < 0.001) return;
    [NSUserDefaults.standardUserDefaults setDouble:clampedStrength forKey:OPNBackgroundTintStrengthDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

uint16_t OpnControllerLibraryShortcutMask(void) {
    id stored = [NSUserDefaults.standardUserDefaults objectForKey:OPNControllerLibraryShortcutDefaultsKey];
    if (![stored respondsToSelector:@selector(integerValue)]) return OPNDefaultControllerLibraryShortcutMask;
    NSInteger value = [stored integerValue];
    return (uint16_t)MAX(0, MIN(value, 0xFFFF));
}

void OpnSetControllerLibraryShortcutMask(uint16_t mask) {
    if (mask == OpnControllerLibraryShortcutMask()) return;
    [NSUserDefaults.standardUserDefaults setInteger:(NSInteger)mask forKey:OPNControllerLibraryShortcutDefaultsKey];
    [NSUserDefaults.standardUserDefaults synchronize];
    [NSNotificationCenter.defaultCenter postNotificationName:OPNInterfacePreferencesDidChangeNotification object:nil];
}

static void OPNAppendLittleEndianUInt16(NSMutableData *data, uint16_t value) {
    uint16_t little = CFSwapInt16HostToLittle(value);
    [data appendBytes:&little length:sizeof(little)];
}

static void OPNAppendLittleEndianUInt32(NSMutableData *data, uint32_t value) {
    uint32_t little = CFSwapInt32HostToLittle(value);
    [data appendBytes:&little length:sizeof(little)];
}

static NSData *OPNConsoleToneWAVData(OPNConsoleTone tone) {
    static NSMutableDictionary<NSNumber *, NSData *> *cache;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        cache = [NSMutableDictionary dictionary];
    });

    NSNumber *key = @(tone);
    NSData *cached = cache[key];
    if (cached) return cached;

    const uint32_t sampleRate = 44100;
    double duration = 0.070;
    double primaryFrequency = 660.0;
    double secondaryFrequency = 990.0;
    double volume = 0.22;
    switch (tone) {
        case OPNConsoleToneMove:
            duration = 0.052;
            primaryFrequency = 720.0;
            secondaryFrequency = 1080.0;
            volume = 0.17;
            break;
        case OPNConsoleToneSelect:
            duration = 0.105;
            primaryFrequency = 620.0;
            secondaryFrequency = 1240.0;
            volume = 0.23;
            break;
        case OPNConsoleToneChange:
            duration = 0.090;
            primaryFrequency = 880.0;
            secondaryFrequency = 1320.0;
            volume = 0.20;
            break;
        case OPNConsoleToneBack:
            duration = 0.080;
            primaryFrequency = 440.0;
            secondaryFrequency = 330.0;
            volume = 0.18;
            break;
    }

    const uint16_t channels = 1;
    const uint16_t bitsPerSample = 16;
    const uint32_t frameCount = (uint32_t)std::round(duration * sampleRate);
    const uint32_t dataByteCount = frameCount * channels * (bitsPerSample / 8);
    NSMutableData *data = [NSMutableData dataWithCapacity:44 + dataByteCount];

    [data appendBytes:"RIFF" length:4];
    OPNAppendLittleEndianUInt32(data, 36 + dataByteCount);
    [data appendBytes:"WAVE" length:4];
    [data appendBytes:"fmt " length:4];
    OPNAppendLittleEndianUInt32(data, 16);
    OPNAppendLittleEndianUInt16(data, 1);
    OPNAppendLittleEndianUInt16(data, channels);
    OPNAppendLittleEndianUInt32(data, sampleRate);
    OPNAppendLittleEndianUInt32(data, sampleRate * channels * (bitsPerSample / 8));
    OPNAppendLittleEndianUInt16(data, channels * (bitsPerSample / 8));
    OPNAppendLittleEndianUInt16(data, bitsPerSample);
    [data appendBytes:"data" length:4];
    OPNAppendLittleEndianUInt32(data, dataByteCount);

    for (uint32_t frame = 0; frame < frameCount; frame++) {
        double t = (double)frame / (double)sampleRate;
        double progress = (double)frame / (double)MAX(1u, frameCount - 1);
        double attack = MIN(1.0, progress / 0.10);
        double release = MIN(1.0, (1.0 - progress) / 0.42);
        double envelope = attack * release;
        double bend = 1.0 + (tone == OPNConsoleToneBack ? -0.18 : 0.10) * (1.0 - progress);
        double sample = sin(2.0 * M_PI * primaryFrequency * bend * t) * 0.68;
        sample += sin(2.0 * M_PI * secondaryFrequency * t) * 0.24;
        sample += sin(2.0 * M_PI * primaryFrequency * 2.0 * t) * 0.08;
        int16_t pcm = (int16_t)std::round(MAX(-1.0, MIN(1.0, sample * envelope * volume)) * 32767.0);
        OPNAppendLittleEndianUInt16(data, (uint16_t)pcm);
    }

    NSData *immutableData = [data copy];
    cache[key] = immutableData;
    return immutableData;
}

void OpnPlayConsoleTone(OPNConsoleTone tone) {
    static NSMutableArray<AVAudioPlayer *> *activePlayers;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        activePlayers = [NSMutableArray array];
    });

    NSError *error = nil;
    AVAudioPlayer *player = [[AVAudioPlayer alloc] initWithData:OPNConsoleToneWAVData(tone) error:&error];
    if (!player || error) return;
    player.volume = 0.85;
    [player prepareToPlay];
    [activePlayers addObject:player];
    [player play];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)((player.duration + 0.25) * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [activePlayers removeObject:player];
    });
}

static unsigned OpnResolvedInterfaceColor(unsigned rgb) {
    unsigned accent = OpnCurrentAccentRGB();
    switch (rgb) {
        case OPN::kBrandGreen: return accent;
        case OPN::kBrandGreenHover: return OpnBlendRGB(accent, 0xFFFFFF, 0.16);
        case OPN::kBrandGreenPress: return OpnBlendRGB(accent, 0x000000, 0.18);
        case OPN::kAccentOn: {
            CGFloat r = ((accent >> 16) & 0xFF) / 255.0;
            CGFloat g = ((accent >> 8) & 0xFF) / 255.0;
            CGFloat b = (accent & 0xFF) / 255.0;
            CGFloat luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            return luminance >= 0.48 ? 0x06140A : 0xF7FFF9;
        }
        default: break;
    }
    return rgb;
}

NSColor *OpnColor(unsigned rgb, CGFloat alpha) {
    rgb = OpnResolvedInterfaceColor(rgb);
    return [NSColor colorWithCalibratedRed:((rgb >> 16) & 0xFF) / 255.0
                                     green:((rgb >> 8) & 0xFF) / 255.0
                                      blue:(rgb & 0xFF) / 255.0
                                     alpha:alpha];
}

NSDictionary<NSAttributedStringKey, id> *OpnTextStyle(CGFloat size, NSColor *color,
                                                       NSFontWeight weight) {
    return @{
        NSFontAttributeName: [NSFont systemFontOfSize:size weight:weight],
        NSForegroundColorAttributeName: color,
    };
}

NSTextField *OpnLabel(NSString *text, NSRect frame, CGFloat size, NSColor *color,
                       NSFontWeight weight, NSTextAlignment alignment) {
    NSTextField *label = [[NSTextField alloc] initWithFrame:frame];
    label.stringValue = text;
    label.font = [NSFont systemFontOfSize:size weight:weight];
    label.textColor = color;
    label.alignment = alignment;
    label.drawsBackground = NO;
    label.bordered = NO;
    label.editable = NO;
    label.selectable = NO;
    return label;
}

NSButton *OpnButton(NSString *title, NSRect frame, NSColor *background, NSColor *textColor,
                     bool bordered, NSColor *borderColor) {
    NSButton *button = [[NSButton alloc] initWithFrame:frame];
    button.title = title;
    button.bezelStyle = NSBezelStyleRegularSquare;
    button.bordered = NO;
    button.focusRingType = NSFocusRingTypeNone;
    button.font = [NSFont systemFontOfSize:14.0 weight:NSFontWeightSemibold];
    button.contentTintColor = textColor;
    button.wantsLayer = YES;
    button.layer.backgroundColor = background.CGColor;
    button.layer.cornerRadius = 10.0;
    if (bordered) {
        button.layer.borderWidth = 1.0;
        button.layer.borderColor = (borderColor ? borderColor : OpnColor(OPN::kBrandGreen)).CGColor;
    }
    return button;
}

NSTextField *OpnTextField(NSRect frame, NSString *placeholder, bool isSecure) {
    NSTextField *field = isSecure
        ? [[NSSecureTextField alloc] initWithFrame:frame]
        : [[NSTextField alloc] initWithFrame:frame];
    field.placeholderString = placeholder;
    field.font = [NSFont systemFontOfSize:14 weight:NSFontWeightRegular];
    field.textColor = OpnColor(OPN::kTextPrimary);
    field.backgroundColor = OpnColor(OPN::kInputBackground);
    field.bordered = YES;
    field.focusRingType = NSFocusRingTypeExterior;
    field.bezelStyle = NSTextFieldRoundedBezel;
    return field;
}

NSProgressIndicator *OpnSpinner(NSRect frame) {
    NSProgressIndicator *spinner = [[NSProgressIndicator alloc] initWithFrame:frame];
    spinner.style = NSProgressIndicatorStyleSpinning;
    spinner.controlSize = NSControlSizeRegular;
    spinner.displayedWhenStopped = NO;
    return spinner;
}

void OpnDisableFocusHighlights(NSView *view) {
    if (!view) return;
    view.focusRingType = NSFocusRingTypeNone;
    for (NSView *subview in view.subviews) {
        OpnDisableFocusHighlights(subview);
    }
}
