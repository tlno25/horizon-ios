#import <Foundation/Foundation.h>
#import "horizon_ios_bridge.h"

// Winit must initialize UIKit. Do not add SwiftUI @main or UIApplicationMain.
int main(int argc, char *argv[]) {
    @autoreleasepool {
        return horizon_ios_run();
    }
}
