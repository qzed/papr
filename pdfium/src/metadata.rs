use std::ffi::{c_void, CString};

use crate::{Document, Library, Result};

pub struct Metadata<'a> {
    lib: &'a Library,
    doc: &'a Document,
}

impl<'a> Metadata<'a> {
    pub(crate) fn new(lib: &'a Library, doc: &'a Document) -> Self {
        Metadata { lib, doc }
    }

    pub fn get(&self, tag: MetadataTag) -> Result<Option<String>> {
        self.get_raw(tag.as_str())
    }

    pub fn get_raw(&self, tag: &str) -> Result<Option<String>> {
        let tag = CString::new(tag).unwrap();

        // get length, including trailing zeros
        let len = unsafe {
            self.lib.ftable().FPDF_GetMetaText(
                self.doc.handle().as_ptr(),
                tag.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
        };

        // zero-length or null-terminator only means metadata entry is not
        // present
        if len <= 2 {
            return Ok(None);
        }

        // get actual string as bytes
        let mut buffer: Vec<u8> = vec![0; len as usize];

        let res = unsafe {
            self.lib.ftable().FPDF_GetMetaText(
                self.doc.handle().as_ptr(),
                tag.as_ptr(),
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as u64,
            )
        };

        assert_eq!(res, len);

        // convert bytes to string
        let value = crate::utils::utf16le_from_bytes(&buffer)?;
        Ok(Some(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataTag {
    Title,
    Author,
    Subject,
    Keywords,
    Creator,
    Producer,
    CreationDate,
    ModDate,
}

impl MetadataTag {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetadataTag::Title => "Title",
            MetadataTag::Author => "Author",
            MetadataTag::Subject => "Subject",
            MetadataTag::Keywords => "Keywords",
            MetadataTag::Creator => "Creator",
            MetadataTag::Producer => "Producer",
            MetadataTag::CreationDate => "CreationDate",
            MetadataTag::ModDate => "ModDate",
        }
    }
}

impl AsRef<str> for MetadataTag {
    fn as_ref(&self) -> &'static str {
        self.as_str()
    }
}
