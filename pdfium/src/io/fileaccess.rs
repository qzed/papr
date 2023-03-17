use crate::Result;

use std::ffi::{c_int, c_uchar, c_ulong, c_void};
use std::io::{Read, Seek, SeekFrom};

pub(crate) struct ReaderAccess {
    inner: Box<FileAccessInner>,
}

trait ReadAndSeek: Read + Seek {}
impl<T> ReadAndSeek for T where T: Read + Seek {}

#[repr(C)]
struct FileAccessInner {
    sys: pdfium_sys::FPDF_FILEACCESS,
    reader: Box<dyn ReadAndSeek>,
}

impl ReaderAccess {
    pub(crate) fn from_reader<R>(mut reader: R) -> Result<Self>
    where
        R: Read + Seek + 'static,
    {
        let file_len = reader.seek(SeekFrom::End(0))?;

        // The C API expects a *mut c_void as parameter. However, trait objects
        // are fat (2x) pointers. So attach the reader to the FPDF_FILEACCESS
        // struct and use a pointer to that for both the FPDF API and our
        // callback.

        let reader: Box<dyn ReadAndSeek> = Box::new(reader);

        let sys = pdfium_sys::FPDF_FILEACCESS {
            m_FileLen: file_len,
            m_GetBlock: Some(fa_get_block),
            m_Param: std::ptr::null_mut(),
        };

        let access = FileAccessInner { sys, reader };

        let mut access = ReaderAccess {
            inner: Box::new(access),
        };

        access.inner.sys.m_Param = &*access.inner as *const _ as *mut c_void;

        Ok(access)
    }

    pub(crate) fn sys_ptr(&mut self) -> *mut pdfium_sys::FPDF_FILEACCESS {
        &self.inner.sys as *const _ as *mut _
    }
}

extern "C" fn fa_get_block(
    param: *mut c_void,
    position: c_ulong,
    buf: *mut c_uchar,
    size: c_ulong,
) -> c_int {
    let access = unsafe { &mut *(param as *mut FileAccessInner) };
    let buf = unsafe { std::slice::from_raw_parts_mut(buf, size as usize) };

    let res = access.reader.seek(SeekFrom::Start(position));
    if res.is_err() {
        return 0;
    }

    access.reader.read(buf).unwrap_or(0) as c_int
}
