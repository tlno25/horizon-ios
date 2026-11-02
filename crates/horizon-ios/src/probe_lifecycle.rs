//! Minimum drawable-state policy shared by the UIKit/Metal integration spike.
//! Zero-size views and suspended apps must not acquire presentation textures.

#[derive(Debug, Default)]
pub struct ProbeLifecycle {
    in_foreground: bool,
    size: Option<(u32, u32)>,
}

impl ProbeLifecycle {
    pub fn resume(&mut self) {
        self.in_foreground = true;
    }

    pub fn suspend(&mut self) {
        self.in_foreground = false;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = (width > 0 && height > 0).then_some((width, height));
    }

    #[must_use]
    pub fn drawable_size(&self) -> Option<(u32, u32)> {
        self.in_foreground.then_some(self.size).flatten()
    }
}
