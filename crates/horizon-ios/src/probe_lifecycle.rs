//! Minimum drawable-state policy shared by the UIKit/Metal integration spike.
//! Zero-size views and suspended apps must not acquire presentation textures.

#[derive(Debug, Default)]
pub struct ProbeLifecycle {
    in_foreground: bool,
    size: Option<(u32, u32)>,
    presented: bool,
}

impl ProbeLifecycle {
    pub fn resume(&mut self) {
        self.in_foreground = true;
        self.presented = false;
    }

    pub fn suspend(&mut self) {
        self.in_foreground = false;
        self.presented = false;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let next = (width > 0 && height > 0).then_some((width, height));
        if self.size != next {
            self.presented = false;
        }
        self.size = next;
    }

    /// Report a frame only after its Metal drawable was submitted/presented.
    /// Do not mark the probe ready while its app is suspended or zero-sized.
    pub fn present_frame(&mut self) -> bool {
        let drawable = self.drawable_size().is_some();
        if drawable {
            self.presented = true;
        }
        drawable
    }

    #[must_use]
    pub fn has_presented_frame(&self) -> bool {
        self.presented && self.drawable_size().is_some()
    }

    #[must_use]
    pub fn drawable_size(&self) -> Option<(u32, u32)> {
        self.in_foreground.then_some(self.size).flatten()
    }
}
