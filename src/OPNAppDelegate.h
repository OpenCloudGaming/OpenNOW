#import <Cocoa/Cocoa.h>
#include "common/OPNAuthTypes.h"

@interface AppDelegate : NSObject <NSApplicationDelegate>
@property (nonatomic, strong) NSWindow *window;
@property (nonatomic, strong) NSView *contentContainer;
@property (nonatomic, assign) OPN::AuthScreen currentScreen;
@property (nonatomic, assign) OPN::AuthCredentials pendingCredentials;
@property (nonatomic, assign) OPN::AuthSession currentSession;
@end
