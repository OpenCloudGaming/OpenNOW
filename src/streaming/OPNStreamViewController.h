#include "OPNStreamTypes.h"
#import <Cocoa/Cocoa.h>

NS_ASSUME_NONNULL_BEGIN

@interface OPNStreamViewController : NSViewController

- (instancetype)initWithGameTitle:(const std::string &)title
                            appId:(const std::string &)appId
                         apiToken:(const std::string &)token
                    accountLinked:(bool)accountLinked
                     selectedStore:(const std::string &)selectedStore;

- (void)setInitialViewFrame:(NSRect)frame;

@property(nonatomic, copy) void (^onStreamEnd)
    (BOOL success, const std::string &errorMessage);

- (void)requestQuitGameConfirmation;
- (void)shutdownForApplicationTermination;

@end

NS_ASSUME_NONNULL_END
