use pdfium::doc::MetadataTag;
use pdfium::{Library, Result};

fn main() -> Result<()> {
    let file = std::env::args_os().nth(1).unwrap();

    let lib = Library::init()?;
    let doc = lib.load_file(file, None)?;

    println!("File:");
    println!("  version: {}", doc.version());
    println!("  pages: {:?}", doc.pages().count());

    let tags = [
        MetadataTag::Title,
        MetadataTag::Subject,
        MetadataTag::Author,
        MetadataTag::CreationDate,
        MetadataTag::Creator,
        MetadataTag::Keywords,
        MetadataTag::ModDate,
        MetadataTag::Producer,
    ];

    println!();
    println!("Metadata:");
    for tag in tags {
        let key = tag.as_str();
        let value = doc.metadata().get(tag)?.unwrap_or_else(|| "<unset>".into());

        println!("  {key}: {value:?} ({})", value.len());
    }

    println!();
    println!("Pages:");
    let pages = doc.pages();
    for i in 0..pages.count() {
        let label = pages.get_label(i)?;

        let page = pages.get(i)?;
        let size = page.size();

        println!(
            "  Page {i}: label: {label:?}, width: {}pt, height: {}pt",
            size.x, size.y
        );
    }

    Ok(())
}
