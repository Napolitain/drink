#import <Foundation/Foundation.h>

enum class InputActivity { Clear, Active, Unavailable };

// Reads Core Audio process metadata. Never opens an audio input.
@interface Microphone : NSObject
- (instancetype)initWithChange:(void (^)(void))changed;
- (InputActivity)sample;
@property(nonatomic, readonly) BOOL needsRetry;
@end
