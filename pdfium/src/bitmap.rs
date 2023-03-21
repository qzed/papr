use crate::bindings::Handle;
use crate::{Error, Library, Result};

use std::ffi::c_void;

pub type BitmapHandle = Handle<pdfium_sys::fpdf_bitmap_t__>;

pub struct Owned;

pub struct Bitmap<Container = Owned> {
    lib: Library,
    handle: BitmapHandle,
    _container: Container,
}

impl Bitmap<Owned> {
    pub fn uninitialized(
        lib: Library,
        width: u32,
        height: u32,
        format: BitmapFormat,
    ) -> Result<Bitmap> {
        let handle = unsafe {
            lib.ftable().FPDFBitmap_CreateEx(
                width as _,
                height as _,
                format.as_i32(),
                std::ptr::null_mut(),
                0,
            )
        };
        let handle = lib.assert_handle(handle)?;

        let bitmap = Bitmap {
            lib,
            handle,
            _container: Owned,
        };

        Ok(bitmap)
    }
}

impl<C> Bitmap<C>
where
    C: std::ops::DerefMut<Target = [u8]>,
{
    pub fn from_buf(
        lib: Library,
        width: u32,
        height: u32,
        format: BitmapFormat,
        buffer: C,
        stride: u32,
    ) -> Result<Bitmap<C>> {
        let mut buffer = buffer;

        // check buffer size
        let expecte_size = height as usize * stride as usize;
        if buffer.len() < expecte_size {
            return Err(Error::InvalidArgument);
        }

        // create bitmap
        let handle = unsafe {
            lib.ftable().FPDFBitmap_CreateEx(
                width as _,
                height as _,
                format.as_i32(),
                buffer.as_mut_ptr() as *mut c_void,
                stride as _,
            )
        };
        let handle = lib.assert_handle(handle)?;

        let bitmap = Bitmap {
            lib,
            handle,
            _container: buffer,
        };

        Ok(bitmap)
    }
}

impl<C> Bitmap<C> {
    pub fn handle(&self) -> &BitmapHandle {
        &self.handle
    }

    pub fn library(&self) -> &Library {
        &self.lib
    }

    pub fn width(&self) -> u32 {
        let handle = self.handle().get();
        unsafe { self.library().ftable().FPDFBitmap_GetWidth(handle) as _ }
    }

    pub fn height(&self) -> u32 {
        let handle = self.handle().get();
        unsafe { self.library().ftable().FPDFBitmap_GetHeight(handle) as _ }
    }

    pub fn stride(&self) -> u32 {
        let handle = self.handle().get();
        unsafe { self.library().ftable().FPDFBitmap_GetStride(handle) as _ }
    }

    pub fn format(&self) -> Option<BitmapFormat> {
        let handle = self.handle().get();
        let format = unsafe { self.library().ftable().FPDFBitmap_GetFormat(handle) };

        BitmapFormat::from_i32(format)
    }

    pub fn buf(&self) -> &[u8] {
        let handle = self.handle().get();

        let len = self.stride() as usize * self.height() as usize;
        let data = unsafe { self.library().ftable().FPDFBitmap_GetBuffer(handle) };

        unsafe { std::slice::from_raw_parts(data as *const u8, len) }
    }

    pub fn buf_mut(&mut self) -> &mut [u8] {
        let handle = self.handle().get();

        let len = self.stride() as usize * self.height() as usize;
        let data = unsafe { self.library().ftable().FPDFBitmap_GetBuffer(handle) };

        unsafe { std::slice::from_raw_parts_mut(data as *mut u8, len) }
    }

    pub fn fill_rect(&mut self, left: u32, top: u32, width: u32, height: u32, color: Color) {
        unsafe {
            self.library().ftable().FPDFBitmap_FillRect(
                self.handle().get(),
                left as _,
                top as _,
                width as _,
                height as _,
                color.as_u32() as _,
            )
        }
    }
}

impl<C> Drop for Bitmap<C> {
    fn drop(&mut self) {
        unsafe { self.lib.ftable().FPDFBitmap_Destroy(self.handle.get()) };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitmapFormat {
    Gray,
    Bgr,
    Bgrx,
    Bgra,
}

impl BitmapFormat {
    fn from_i32(value: i32) -> Option<Self> {
        match value as u32 {
            pdfium_sys::FPDFBitmap_Gray => Some(BitmapFormat::Gray),
            pdfium_sys::FPDFBitmap_BGR => Some(BitmapFormat::Bgr),
            pdfium_sys::FPDFBitmap_BGRx => Some(BitmapFormat::Bgrx),
            pdfium_sys::FPDFBitmap_BGRA => Some(BitmapFormat::Bgra),
            _ => None,
        }
    }

    fn as_i32(&self) -> i32 {
        match self {
            BitmapFormat::Gray => pdfium_sys::FPDFBitmap_Gray as _,
            BitmapFormat::Bgr => pdfium_sys::FPDFBitmap_BGR as _,
            BitmapFormat::Bgrx => pdfium_sys::FPDFBitmap_BGRx as _,
            BitmapFormat::Bgra => pdfium_sys::FPDFBitmap_BGRA as _,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color::new_rgba(0, 0, 0, 0);
    pub const WHITE: Color = Color::new_rgb(255, 255, 255);
    pub const BLACK: Color = Color::new_rgb(0, 0, 0);

    pub const fn new_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn new_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    fn as_u32(&self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorScheme {
    pub path_fill_color: Color,
    pub path_stroke_color: Color,
    pub text_fill_color: Color,
    pub text_stroke_color: Color,
}

impl From<ColorScheme> for pdfium_sys::FPDF_COLORSCHEME {
    fn from(other: ColorScheme) -> Self {
        pdfium_sys::FPDF_COLORSCHEME {
            path_fill_color: other.path_fill_color.as_u32() as _,
            path_stroke_color: other.path_stroke_color.as_u32() as _,
            text_fill_color: other.text_fill_color.as_u32() as _,
            text_stroke_color: other.text_stroke_color.as_u32() as _,
        }
    }
}
