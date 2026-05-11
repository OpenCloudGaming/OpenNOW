#import <Cocoa/Cocoa.h>
#include <functional>
#include "../common/OPNGameTypes.h"

@interface OPNGameCatalogView : NSView <NSSearchFieldDelegate>

@property (nonatomic, copy) void (^onSelectGame)(const OPN::GameInfo &game, int variantIndex);
@property (nonatomic, copy) void (^onSignOut)();
@property (nonatomic, copy) void (^onGameCountChanged)(NSInteger count);
@property (nonatomic, copy) void (^onCatalogBrowseRequested)(NSString *searchQuery, NSString *sortId, const std::vector<std::string> &filterIds);

- (instancetype)initWithFrame:(NSRect)frame;
- (void)setGames:(const std::vector<OPN::GameInfo> &)games;
- (void)setCatalogBrowseResult:(const OPN::CatalogBrowseResult &)result;
- (void)setLoading:(BOOL)loading;
- (void)setError:(NSString *)message;
- (void)setUserName:(NSString *)name;

@end
