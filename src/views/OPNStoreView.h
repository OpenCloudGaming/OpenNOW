#import <Cocoa/Cocoa.h>
#include <functional>
#include <vector>
#include "../common/OPNGameTypes.h"

@interface OPNStoreView : NSView

@property (nonatomic, copy) void (^onSelectGame)(const OPN::GameInfo &game, int variantIndex);

- (instancetype)initWithFrame:(NSRect)frame;
- (void)setLoading:(BOOL)loading;
- (void)setError:(NSString *)message;
- (void)setLibraryGames:(const std::vector<OPN::GameInfo> &)games;
- (void)setPanels:(const std::vector<OPN::PanelResult> &)panels;

@end
