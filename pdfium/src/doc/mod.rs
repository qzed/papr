mod document;
mod metadata;
mod page;
mod version;

pub use document::{Document, DocumentHandle};
pub use metadata::{Metadata, MetadataTag};
pub use page::{Page, PageHandle, PageRenderLayout, PageRotation, Pages, RenderFlags};
pub use version::Version;

pub(crate) use document::DocumentBacking;
