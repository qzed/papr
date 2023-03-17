#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl From<pdfium_sys::FS_RECTF> for Rect {
    fn from(other: pdfium_sys::FS_RECTF) -> Self {
        Rect {
            left: other.left,
            top: other.top,
            right: other.right,
            bottom: other.bottom,
        }
    }
}
