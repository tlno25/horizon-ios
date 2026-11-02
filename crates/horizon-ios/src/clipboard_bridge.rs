//! Native clipboard reads are length-delimited and must never expose Rust allocations.
//!
//! iOS callbacks can decline off-main-thread calls; the shared SystemClipboard
//! then uses the game's existing in-memory fallback.

const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

/// Read UTF-8 bytes from a two-call platform API. The length must remain stable;
/// a changed clipboard, malformed UTF-8 or excessively large value is rejected.
fn read_utf8(read: &mut impl FnMut(*mut u8, usize) -> usize) -> Option<String> {
    let length = read(std::ptr::null_mut(), 0);
    if length > MAX_CLIPBOARD_BYTES {
        return None;
    }
    if length == 0 {
        return Some(String::new());
    }
    let mut bytes = vec![0; length];
    (read(bytes.as_mut_ptr(), length) == length)
        .then(|| String::from_utf8(bytes).ok())
        .flatten()
}

#[cfg(target_os = "ios")]
unsafe extern "C" {
    /// usize::MAX means the caller must use the local clipboard fallback.
    fn horizon_ios_clipboard_read(destination: *mut u8, capacity: usize) -> usize;
    fn horizon_ios_clipboard_write(bytes: *const u8, length: usize);
}

#[cfg(target_os = "ios")]
fn platform_get() -> Option<String> {
    read_utf8(&mut |destination, capacity| unsafe {
        horizon_ios_clipboard_read(destination, capacity)
    })
}

#[cfg(target_os = "ios")]
fn platform_set(text: &str) {
    if text.len() <= MAX_CLIPBOARD_BYTES {
        unsafe { horizon_ios_clipboard_write(text.as_ptr(), text.len()) };
    }
}

#[cfg(target_os = "ios")]
pub(super) fn install() {
    horizon_client_input::clipboard::install_clipboard(
        horizon_client_input::clipboard::ClipboardHooks {
            get: platform_get,
            set: platform_set,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_utf8_without_requiring_a_null_terminator() {
        let mut read = |destination: *mut u8, capacity: usize| {
            let bytes = "日本語 clipboard".as_bytes();
            if !destination.is_null() && capacity >= bytes.len() {
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()) };
            }
            bytes.len()
        };
        assert_eq!(read_utf8(&mut read), Some("日本語 clipboard".into()));
    }

    #[test]
    fn refuses_oversized_or_racy_platform_buffers() {
        assert_eq!(read_utf8(&mut |_, _| usize::MAX), None);
        assert_eq!(read_utf8(&mut |_, _| 1_048_577), None);
        let mut calls = 0;
        let mut changed = |destination: *mut u8, _capacity: usize| {
            calls += 1;
            if !destination.is_null() {
                unsafe { *destination = b'x' };
                2
            } else {
                1
            }
        };
        assert_eq!(read_utf8(&mut changed), None);
    }

    #[test]
    fn invalid_utf8_is_not_passed_to_game() {
        assert_eq!(
            read_utf8(&mut |destination, _| {
                if !destination.is_null() {
                    unsafe { *destination = 0xff };
                }
                1
            }),
            None
        );
    }
}
