use crate::bitmap::{Bitmap, ColorScheme};
use crate::doc::{Page, PageRenderLayout, RenderFlags};
use crate::{Error, Result};

use std::ffi::{c_int, c_void};
use std::panic::AssertUnwindSafe;

pub struct ProgressiveRender<'a, 'b, C, F> {
    page: &'a Page,
    bitmap: &'b Bitmap<C>,
    status: ProgressiveRenderStatus,
    should_pause: F,
    closed: bool,
}

impl<'a, 'b, C, F> ProgressiveRender<'a, 'b, C, F> {
    pub(crate) fn new(
        page: &'a Page,
        bitmap: &'b Bitmap<C>,
        status: ProgressiveRenderStatus,
        should_pause: F,
    ) -> ProgressiveRender<'a, 'b, C, F> {
        ProgressiveRender {
            page,
            bitmap,
            status,
            should_pause,
            closed: false,
        }
    }

    pub fn page(&self) -> &'a Page {
        self.page
    }

    pub fn bitmap(&self) -> &'b Bitmap<C> {
        self.bitmap
    }

    pub fn status(&self) -> ProgressiveRenderStatus {
        self.status
    }

    pub fn render_continue(&mut self) -> Result<ProgressiveRenderStatus>
    where
        F: FnMut() -> bool,
    {
        self.status = render_continue(self.page, &mut self.should_pause)?;
        Ok(self.status)
    }

    pub fn render_finish(&mut self) -> Result<()> {
        self.status = render_finish(self.page)?;
        assert_eq!(self.status, ProgressiveRenderStatus::Complete);

        Ok(())
    }

    pub fn render_close(&mut self) {
        if !self.closed {
            render_close(self.page);
            self.closed = true;
        }
    }
}

impl<'a, 'b, C, F> Drop for ProgressiveRender<'a, 'b, C, F> {
    fn drop(&mut self) {
        self.render_close()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressiveRenderStatus {
    Incomplete,
    Complete,
}

impl ProgressiveRenderStatus {
    fn try_from_raw(status: c_int) -> Result<Self> {
        match status as _ {
            pdfium_sys::FPDF_RENDER_READY => {
                // This state is generally not exposed. We cannot directly
                // query the status (only get it as return value of a
                // render call), at which point it will be one of the
                // values below.
                panic!("unexpected progressive render state: ready")
            }
            pdfium_sys::FPDF_RENDER_FAILED => {
                // This status generally does not indicate an underlying
                // library failure, but rather an invalid invocation of one
                // of the render functions. For example, calling
                // render_continue() without a previous render_start()
                // call.
                Err(Error::InvalidOperation)
            }
            pdfium_sys::FPDF_RENDER_TOBECONTINUED => Ok(ProgressiveRenderStatus::Incomplete),
            pdfium_sys::FPDF_RENDER_DONE => Ok(ProgressiveRenderStatus::Complete),
            _ => {
                // There should not be any other status.
                panic!("progressive render returned unexpected status code: {status}")
            }
        }
    }
}

pub fn render_start<C, F>(
    page: &Page,
    bitmap: &mut Bitmap<C>,
    layout: &PageRenderLayout,
    flags: RenderFlags,
    should_pause: F,
) -> Result<ProgressiveRenderStatus>
where
    F: FnMut() -> bool,
{
    // set up callback trampoline
    let mut cbdata = CallbackData::new(should_pause);

    let mut pause = pdfium_sys::IFSDK_PAUSE {
        version: 1,
        NeedToPauseNow: Some(cbdata.trampoline()),
        user: &mut cbdata as *mut CallbackData<_> as *mut c_void,
    };

    // start render
    let status = unsafe {
        page.library().ftable().FPDF_RenderPageBitmap_Start(
            bitmap.handle().as_ptr(),
            page.handle().as_ptr(),
            layout.start.x,
            layout.start.y,
            layout.size.x,
            layout.size.y,
            layout.rotate.as_i32(),
            flags.bits() as _,
            &mut pause,
        )
    };

    // check for panic in callback
    if let Err(err) = cbdata.panic {
        std::panic::resume_unwind(err)
    }

    // check for error in render call
    page.library().assert_status()?;

    ProgressiveRenderStatus::try_from_raw(status)
}

pub fn render_with_colorscheme_start<C, F>(
    page: &Page,
    bitmap: &mut Bitmap<C>,
    layout: &PageRenderLayout,
    flags: RenderFlags,
    colors: &ColorScheme,
    should_pause: F,
) -> Result<ProgressiveRenderStatus>
where
    F: FnMut() -> bool,
{
    let colors = (*colors).into();

    // set up callback trampoline
    let mut cbdata = CallbackData::new(should_pause);

    let mut pause = pdfium_sys::IFSDK_PAUSE {
        version: 1,
        NeedToPauseNow: Some(cbdata.trampoline()),
        user: &mut cbdata as *mut CallbackData<_> as *mut c_void,
    };

    // start render
    let status = unsafe {
        page.library()
            .ftable()
            .FPDF_RenderPageBitmapWithColorScheme_Start(
                bitmap.handle().as_ptr(),
                page.handle().as_ptr(),
                layout.start.x,
                layout.start.y,
                layout.size.x,
                layout.size.y,
                layout.rotate.as_i32(),
                flags.bits() as _,
                &colors,
                &mut pause,
            )
    };

    // check for panic in callback
    if let Err(err) = cbdata.panic {
        std::panic::resume_unwind(err)
    }

    // check for error in render call
    page.library().assert_status()?;

    ProgressiveRenderStatus::try_from_raw(status)
}

pub fn render_continue<F>(page: &Page, should_pause: F) -> Result<ProgressiveRenderStatus>
where
    F: FnMut() -> bool,
{
    // set up callback trampoline
    let mut cbdata = CallbackData::new(should_pause);

    let mut pause = pdfium_sys::IFSDK_PAUSE {
        version: 1,
        NeedToPauseNow: Some(cbdata.trampoline()),
        user: &mut cbdata as *mut CallbackData<_> as *mut c_void,
    };

    // continue render
    let status = unsafe {
        page.library()
            .ftable()
            .FPDF_RenderPage_Continue(page.handle().as_ptr(), &mut pause)
    };

    // check for panic in callback
    if let Err(err) = cbdata.panic {
        std::panic::resume_unwind(err)
    }

    // check for error in render call
    page.library().assert_status()?;

    ProgressiveRenderStatus::try_from_raw(status)
}

pub fn render_finish(page: &Page) -> Result<ProgressiveRenderStatus> {
    // continue render
    let status = unsafe {
        page.library()
            .ftable()
            .FPDF_RenderPage_Continue(page.handle().as_ptr(), std::ptr::null_mut())
    };

    // check for error in render call
    page.library().assert_status()?;

    ProgressiveRenderStatus::try_from_raw(status)
}

pub fn render_close(page: &Page) {
    unsafe {
        page.library()
            .ftable()
            .FPDF_RenderPage_Close(page.handle().as_ptr());
    }
}

struct CallbackData<C> {
    closure: C,
    panic: std::thread::Result<()>,
}

impl<C> CallbackData<C>
where
    C: FnMut() -> bool,
{
    fn new(closure: C) -> Self {
        Self {
            closure,
            panic: Ok(()),
        }
    }

    fn trampoline(&self) -> Trampoline {
        trampoline::<C>
    }
}

type Trampoline = unsafe extern "C" fn(*mut pdfium_sys::IFSDK_PAUSE) -> i32;

unsafe extern "C" fn trampoline<C>(param: *mut pdfium_sys::IFSDK_PAUSE) -> c_int
where
    C: FnMut() -> bool,
{
    let data: &mut CallbackData<C> = &mut *((*param).user as *mut CallbackData<C>);

    let result = std::panic::catch_unwind(AssertUnwindSafe(&mut data.closure));
    let should_pause = match result {
        Ok(should_pause) => {
            data.panic = Ok(());
            should_pause
        }
        Err(err) => {
            data.panic = Err(err);
            true
        }
    };

    should_pause as c_int
}
