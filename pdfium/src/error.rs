use std::ffi::c_ulong;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid encoding")]
    InvalidEncoding,

    #[error("Error accessing shared library")]
    LibraryError(#[from] libloading::Error),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    ErrorCode(#[from] ErrorCode),
}

#[derive(Error, Debug)]
pub enum ErrorCode {
    #[error("Unknown error")]
    Unknown,

    #[error("File not found or could not be opened")]
    File,

    #[error("File not in PDF format or corrupted")]
    Format,

    #[error("Password required or incorrect password")]
    Password,

    #[error("Unsupported security scheme")]
    Security,

    #[error("Page not found or content error")]
    Page,

    #[error("Load XFA error")]
    XfaLoad,

    #[error("Layout XFA error")]
    XfaLayout,

    #[error("Unsupported error code")]
    Invalid,
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn error_code_to_result(err: c_ulong) -> std::result::Result<(), ErrorCode> {
    match err as u32 {
        pdfium_sys::FPDF_ERR_SUCCESS => Ok(()),
        pdfium_sys::FPDF_ERR_UNKNOWN => Err(ErrorCode::Unknown),
        pdfium_sys::FPDF_ERR_FILE => Err(ErrorCode::File),
        pdfium_sys::FPDF_ERR_FORMAT => Err(ErrorCode::Format),
        pdfium_sys::FPDF_ERR_PASSWORD => Err(ErrorCode::Password),
        pdfium_sys::FPDF_ERR_SECURITY => Err(ErrorCode::Security),
        pdfium_sys::FPDF_ERR_PAGE => Err(ErrorCode::Page),
        pdfium_sys::FPDF_ERR_XFALOAD => Err(ErrorCode::XfaLoad),
        pdfium_sys::FPDF_ERR_XFALAYOUT => Err(ErrorCode::XfaLayout),
        _ => Err(ErrorCode::Invalid),
    }
}
