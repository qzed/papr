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

/// Matrix for transformation in the form `[a b c d e f]` as specified in PDF
/// 32000-1:2008 (version 1.7), Section 8.3.
///
/// Examples for basic transformations:
/// - Translation: `[1 0 0 1 tx ty]`
/// - Scaling: `[sx 0 0 sy 0 0]`
/// - Rotation: `[cos(q) sin(q) -sin(q) cos(q) 0 0]`
///
/// Transformations are computed as:
/// ```txt
///                       ⎡a b 0⎤
/// [x' y' 1] = [x y 1] * ⎢c d 0⎥
///                       ⎣e f 1⎦
/// ```
/// or
/// ```txt
/// ⎡x'⎤   ⎡a c e⎤   ⎡x⎤
/// ⎢y'⎥ = ⎢b d f⎥ * ⎢y⎥
/// ⎣1 ⎦   ⎣0 0 1⎦   ⎣1⎦
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl From<pdfium_sys::FS_MATRIX> for Matrix {
    fn from(other: pdfium_sys::FS_MATRIX) -> Self {
        Self {
            a: other.a,
            b: other.b,
            c: other.c,
            d: other.d,
            e: other.e,
            f: other.f,
        }
    }
}

impl From<Matrix> for pdfium_sys::FS_MATRIX {
    fn from(other: Matrix) -> Self {
        Self {
            a: other.a,
            b: other.b,
            c: other.c,
            d: other.d,
            e: other.e,
            f: other.f,
        }
    }
}

impl From<Matrix> for Affine2<f32> {
    fn from(m: Matrix) -> Self {
        Affine2::from_matrix_unchecked(matrix![
            m.a, m.c, m.e;
            m.b, m.d, m.f;
            0.0, 0.0, 1.0;
        ])
    }
}

impl From<Affine2<f32>> for Matrix {
    fn from(m: Affine2<f32>) -> Self {
        Matrix {
            a: m[(0, 0)],
            b: m[(1, 0)],
            c: m[(0, 1)],
            d: m[(1, 1)],
            e: m[(0, 2)],
            f: m[(1, 2)],
        }
    }
}
