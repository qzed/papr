use nalgebra::matrix;

pub use nalgebra::{Affine2, Point2, Vector2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl From<pdfium_sys::FS_RECTF> for Rect {
    fn from(other: pdfium_sys::FS_RECTF) -> Self {
        Self {
            left: other.left,
            top: other.top,
            right: other.right,
            bottom: other.bottom,
        }
    }
}

impl From<Rect> for pdfium_sys::FS_RECTF {
    fn from(other: Rect) -> Self {
        Self {
            left: other.left,
            top: other.top,
            right: other.right,
            bottom: other.bottom,
        }
    }
}

pub fn affine_from_pdfmatrix(m: &pdfium_sys::FS_MATRIX) -> Affine2<f32> {
    Affine2::from_matrix_unchecked(matrix![
        m.a, m.c, m.e;
        m.b, m.d, m.f;
        0.0, 0.0, 1.0;
    ])
}

pub fn affine_to_pdfmatrix(m: &Affine2<f32>) -> pdfium_sys::FS_MATRIX {
    pdfium_sys::FS_MATRIX {
        a: m[(0, 0)],
        b: m[(1, 0)],
        c: m[(0, 1)],
        d: m[(1, 1)],
        e: m[(0, 2)],
        f: m[(1, 2)],
    }
}
