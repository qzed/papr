use nalgebra::{Point2, Vector2};

/// Page rotation used for rendering.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PageRotation {
    /// Do not rotate.
    None,

    /// Rotate 90 degrees clockwise.
    Deg90,

    /// Rotate 180 degrees clockwise.
    Deg180,

    /// Rotate 270 degrees clockwise.
    Deg270,
}

impl PageRotation {
    pub(crate) fn as_i32(&self) -> i32 {
        match self {
            PageRotation::None => 0,
            PageRotation::Deg90 => 1,
            PageRotation::Deg180 => 2,
            PageRotation::Deg270 => 3,
        }
    }
}

/// Descriptor for the page/viewport layout used for rendering.
pub struct PageRenderLayout {
    /// Offset of the display/viewport on the page, in pixels.
    pub start: Point2<i32>,

    /// Size of the full page to be rendered, in pixels.
    pub size: Vector2<i32>,

    /// Rotation of the page.
    pub rotate: PageRotation,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct RenderFlags: u32 {
        /// Render annotations
        const Annotations = pdfium_sys::FPDF_ANNOT;

        /// Use text rendering optimized for LCD displays. This flag will only
        /// take effect if anti-aliasing is enabled for text.
        const LcdText = pdfium_sys::FPDF_LCD_TEXT;

        /// Don't use the native text output available on some platforms.
        const NoNativeText = pdfium_sys::FPDF_NO_NATIVETEXT;

        /// Grayscale output.
        const Grayscale = pdfium_sys::FPDF_GRAYSCALE;

        /// Limit image cache size.
        const LimitImageCache = pdfium_sys::FPDF_RENDER_LIMITEDIMAGECACHE;

        /// Always use halftone for image stretching.
        const ForceHalftone = pdfium_sys::FPDF_RENDER_FORCEHALFTONE;

        /// Render for printing.
        const Print = pdfium_sys::FPDF_PRINTING;

        /// Disable anti-aliasing on text. This flag will also disable LCD
        /// optimization for text rendering.
        const NoSmoothText = pdfium_sys::FPDF_RENDER_NO_SMOOTHTEXT;

        /// Disable anti-aliasing on images.
        const NoSmoothImage = pdfium_sys::FPDF_RENDER_NO_SMOOTHIMAGE;

        /// Set to disable anti-aliasing on paths.
        const NoSmoothPath = pdfium_sys::FPDF_RENDER_NO_SMOOTHPATH;

        /// Set whether to render in a reverse Byte order, this flag is only
        /// used when rendering to a bitmap.
        const ReverseByteOrder = pdfium_sys::FPDF_REVERSE_BYTE_ORDER;

        /// Whether fill paths need to be stroked. This flag is only used when
        /// a color scheme is passed in, since with a single fill color for
        /// paths the boundaries of adjacent fill paths are less visible.
        const ConvertFillToStroke = pdfium_sys::FPDF_CONVERT_FILL_TO_STROKE;
    }
}
