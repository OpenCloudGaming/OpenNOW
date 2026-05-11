#pragma once

#import <Cocoa/Cocoa.h>

NSColor *OpnColor(unsigned rgb, CGFloat alpha = 1.0);

extern NSString *const OPNInterfacePreferencesDidChangeNotification;

unsigned OpnCurrentAccentRGB(void);
void OpnSetCurrentAccentRGB(unsigned rgb);
CGFloat OpnPosterSizeScale(void);
void OpnSetPosterSizeScale(CGFloat scale);
BOOL OpnAutoFullScreenEnabled(void);
void OpnSetAutoFullScreenEnabled(BOOL enabled);
BOOL OpnControllerModeEnabled(void);
void OpnSetControllerModeEnabled(BOOL enabled);

NSDictionary<NSAttributedStringKey, id> *OpnTextStyle(CGFloat size, NSColor *color,
                                                        NSFontWeight weight = NSFontWeightRegular);

NSTextField *OpnLabel(NSString *text, NSRect frame, CGFloat size, NSColor *color,
                       NSFontWeight weight = NSFontWeightRegular,
                       NSTextAlignment alignment = NSTextAlignmentLeft);

NSButton *OpnButton(NSString *title, NSRect frame, NSColor *background, NSColor *textColor,
                     bool bordered = false, NSColor *borderColor = nil);

NSTextField *OpnTextField(NSRect frame, NSString *placeholder, bool isSecure = false);

NSProgressIndicator *OpnSpinner(NSRect frame);
