#include "OPNStreamTypes.h"
#import <Cocoa/Cocoa.h>

NS_ASSUME_NONNULL_BEGIN

@interface OPNStreamViewController : NSViewController

- (instancetype)initWithGameTitle:(const std::string &)title
                             appId:(const std::string &)appId
                          apiToken:(const std::string &)token
                     accountLinked:(bool)accountLinked
                      selectedStore:(const std::string &)selectedStore;

- (instancetype)initWithGameTitle:(const std::string &)title
                             appId:(const std::string &)appId
                          apiToken:(const std::string &)token
                     accountLinked:(bool)accountLinked
                     selectedStore:(const std::string &)selectedStore
                   resumeSessionId:(const std::string &)resumeSessionId
                       resumeServer:(const std::string &)resumeServer;

- (void)setInitialViewFrame:(NSRect)frame;
- (void)startStreamIfNeeded;

@property(nonatomic, copy) void (^onStreamEnd)
    (BOOL success, const std::string &errorMessage);
@property(nonatomic, copy) void (^onControllerLibraryRequested)(void);

- (void)requestQuitGameConfirmation;
- (void)shutdownForApplicationTermination;
- (void)prepareForLibraryPictureInPicture;
- (void)restoreFromLibraryPictureInPicture;
- (NSView *)streamPictureInPictureView;

@end

NS_ASSUME_NONNULL_END
