pub struct Document {
    pub pdf: pdfium::doc::Document,
}

impl Document {
    pub fn load_bytes(bytes: Vec<u8>) -> pdfium::Result<Document> {
        let lib = pdfium::Library::init()?;
        let pdf = lib.load_buffer(bytes, None)?;

        let doc = Document { pdf };
        Ok(doc)
    }
}
