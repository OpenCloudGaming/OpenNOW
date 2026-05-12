#import "OPNUIHelpers.h"
#import "OPNColorTokens.h"
#include <cmath>

NSString *const OPNInterfacePreferencesDidChangeNotification = @"OpenNOW.InterfacePreferencesDidChange";

static NSString *const OPNAccentRedDefaultsKey = @"OpenNOW.Interface.AccentRed";
static NSString *const OPNAccentGreenDefaultsKey = @"OpenNOW.Interface.AccentGreen";
static NSString *const OPNAccentBlueDefaultsKey = @"OpenNOW.Interface.AccentBlue";
static NSString *const OPNPosterSizeScaleDefaultsKey = @"OpenNOW.Interface.PosterSizeScale";
static NSString *const OPNAutoFullScreenDefaultsKey = @"OpenNOW.Interface.AutoFullScreen";
static NSString *const OPNControllerModeDefaultsKey = @"OpenNOW.Interface.ControllerMode";
static const CGFloat OPNMinimumPosterSizeScale = 0.80;
static const CGFloat OPNMaximumPosterSizeScale = 1.30;
static const unsigned OPNDefaultAccentRGB = 0x7CF1B1;

static int OPNClampedColorByte(NSInteger value) {
    return (int)MAX(0, MIN(value, 255));
}

static unsigned OPNBlendRGB(unsigned rgb, unsigned target, CGFloat amount) {
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

static unsigned OpnResolvedInterfaceColor(unsigned rgb) {
    unsigned accent = OpnCurrentAccentRGB();
    switch (rgb) {
        case OPN::kBrandGreen: return accent;
        case OPN::kBrandGreenHover: return OPNBlendRGB(accent, 0xFFFFFF, 0.16);
        case OPN::kBrandGreenPress: return OPNBlendRGB(accent, 0x000000, 0.18);
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
