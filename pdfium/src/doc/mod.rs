mod document;
mod metadata;
mod page;
mod pages;
mod version;

pub use document::{Document, DocumentHandle};
pub use metadata::{Metadata, MetadataTag};
pub use page::{
    Page, PageHandle, PageRenderLayout, PageRotation, ProgressiveRender, ProgressiveRenderStatus,
    RenderFlags,
};
pub use pages::Pages;
pub use version::Version;

pub(crate) use document::DocumentBacking;
