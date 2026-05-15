#pragma once

#import <Cocoa/Cocoa.h>
#include <stdint.h>

NSColor *OpnColor(unsigned rgb, CGFloat alpha = 1.0);
unsigned OpnBlendRGB(unsigned rgb, unsigned target, CGFloat amount);

extern NSString *const OPNInterfacePreferencesDidChangeNotification;

unsigned OpnCurrentAccentRGB(void);
void OpnSetCurrentAccentRGB(unsigned rgb);
CGFloat OpnPosterSizeScale(void);
void OpnSetPosterSizeScale(CGFloat scale);
CGFloat OpnControllerGridItemScale(void);
void OpnSetControllerGridItemScale(CGFloat scale);
BOOL OpnAutoFullScreenEnabled(void);
void OpnSetAutoFullScreenEnabled(BOOL enabled);
BOOL OpnControllerModeEnabled(void);
void OpnSetControllerModeEnabled(BOOL enabled);
BOOL OpnBackgroundAnimationEnabled(void);
void OpnSetBackgroundAnimationEnabled(BOOL enabled);
BOOL OpnDerivedAccentColorsEnabled(void);
void OpnSetDerivedAccentColorsEnabled(BOOL enabled);
CGFloat OpnBackgroundTintStrength(void);
void OpnSetBackgroundTintStrength(CGFloat strength);
uint16_t OpnControllerLibraryShortcutMask(void);
void OpnSetControllerLibraryShortcutMask(uint16_t mask);

typedef NS_ENUM(NSInteger, OPNConsoleTone) {
    OPNConsoleToneMove = 0,
    OPNConsoleToneSelect = 1,
    OPNConsoleToneChange = 2,
    OPNConsoleToneBack = 3,
};

void OpnPlayConsoleTone(OPNConsoleTone tone);

NSDictionary<NSAttributedStringKey, id> *OpnTextStyle(CGFloat size, NSColor *color,
                                                        NSFontWeight weight = NSFontWeightRegular);

NSTextField *OpnLabel(NSString *text, NSRect frame, CGFloat size, NSColor *color,
                       NSFontWeight weight = NSFontWeightRegular,
                       NSTextAlignment alignment = NSTextAlignmentLeft);

NSButton *OpnButton(NSString *title, NSRect frame, NSColor *background, NSColor *textColor,
                     bool bordered = false, NSColor *borderColor = nil);

NSTextField *OpnTextField(NSRect frame, NSString *placeholder, bool isSecure = false);

NSProgressIndicator *OpnSpinner(NSRect frame);

void OpnDisableFocusHighlights(NSView *view);
