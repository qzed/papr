# PDF Annotator Prototype

_Work in progress._

## Status:

- A very basic PDF viewer.
  Can open and render PDFs, but does not yet allow for any kind of interaction beyond that.

## Goal

In short, the idea is to create an application that allows for note-taking in PDF documents on touch/pen devices with the capability of editing and revisiting these notes later on, storing them as "true" PDF annotations following the PDF specification.

More specifically, the goal is the creation of a flexible touch/stylus oriented PDF annotation tool that can handle all kinds of annotations, but mainly focuses on ink-type annotations. In particular, allowing them to be made via a pressure-sensitive stylus.
Unlike other applications, for example Xournal++ or Rnote, the focus is exclusively on the PDF format, not treating it as a secondary import/export target.
This means that annotations should be stored as actual PDF annotations, recognizable as such by other viewers/editors, and not "flattened" into the PDF.

## Requirements

This project uses `pdfium`.
Therefore, you will need a `pdfium` shared library in the search path.
Prebuilt binaries are available at https://github.com/bblanchon/pdfium-binaries.
Note that you can temporarily expand the search path by having `LD_LIBRARY_PATH` point to the pdfium's `lib` directory before invocation if no binaries are installed on the system.

## Running

Run via `cargo run -- path/to/file.pdf`.
