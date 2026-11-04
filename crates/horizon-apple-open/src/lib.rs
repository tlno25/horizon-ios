//! Safe API for opening external websites from Horizon's UIKit event loop.
//! Winit owns UIApplicationMain; do not start a second UIKit application.

/// Only fully qualified web URLs may leave the app. In particular, do not
/// treat a javascript:, file:, or custom scheme as an external website.
pub fn is_web_url(url: &str) -> bool {
    url::Url::parse(url)
        .is_ok_and(|parsed| matches!(parsed.scheme(), "https" | "http") && parsed.has_host())
}

/// Submit an http(s) URL to UIKit on its main thread. Returning `true`
/// confirms the request was submitted, not that Safari finished opening.
#[cfg(target_os = "ios")]
pub fn open_web_url(url: &str) -> bool {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{MainThreadMarker, NSDictionary, NSString, NSURL};
    use objc2_ui_kit::UIApplication;

    if !is_web_url(url) {
        return false;
    }
    let Some(main_thread) = MainThreadMarker::new() else {
        return false;
    };
    // SAFETY: NSString holds a valid UTF-8 URL, NSURL copies/retains it,
    // and the returned Objective-C object is managed by the binding.
    let Some(native_url) = (unsafe { NSURL::URLWithString(&NSString::from_str(url)) }) else {
        return false;
    };
    let options = NSDictionary::<NSString, AnyObject>::new();
    let application = UIApplication::sharedApplication(main_thread);
    // SAFETY: UIKit is accessed on its main thread and all ObjC objects
    // remain alive through this synchronous invocation. No completion block
    // is required. Winit continues to own the app lifecycle and event loop.
    unsafe { application.openURL_options_completionHandler(&native_url, &options, None) };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_absolute_websites_only() {
        assert!(is_web_url("https://playhorizon.dev"));
        assert!(is_web_url("http://example.com/path?x=1"));
        assert!(is_web_url("https://[::1]:443/a"));
    }

    #[test]
    fn rejects_local_scripts_relative_paths_and_invalid_websites() {
        for url in [
            "",
            "//example.com",
            "example.com",
            "https://",
            "https:///",
            "javascript:alert(1)",
            "file:///private/var/mobile/Containers/",
            "data:text/html,a",
            "abc://def",
            "https://space name.test",
        ] {
            assert!(!is_web_url(url), "unexpectedly valid: {url}");
        }
    }
}
