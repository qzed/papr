use std::cell::RefCell;

use adw::subclass::prelude::AdwApplicationWindowImpl;
use gtk::gio::{File, ListStore, SimpleAction};
use gtk::glib::clone;
use gtk::glib::subclass::InitializingObject;
use gtk::prelude::{ActionMapExt, FileExt, StaticType};
use gtk::subclass::prelude::{
    ApplicationWindowImpl, CompositeTemplateClass, CompositeTemplateInitializingExt, ObjectImpl,
    ObjectImplExt, ObjectSubclass, ObjectSubclassExt, WidgetImpl, WindowImpl,
};
use gtk::subclass::widget::WidgetClassSubclassExt;
use gtk::traits::GtkWindowExt;
use gtk::{glib, CompositeTemplate, FileDialog, FileFilter, TemplateChild};
use nalgebra::vector;

use crate::ui::canvas::CanvasWidget;
use crate::ui::viewport::ViewportWidget;

#[derive(CompositeTemplate, Default)]
#[template(resource = "/io/mxnluz/papr/ui/appwindow.ui")]
pub struct AppWindow {
    #[template_child]
    overlay: TemplateChild<adw::ToastOverlay>,

    #[template_child]
    viewport: TemplateChild<ViewportWidget>,

    #[template_child]
    canvas: TemplateChild<CanvasWidget>,

    #[template_child]
    window_title: TemplateChild<adw::WindowTitle>,

    pdflib: RefCell<Option<pdfium::Library>>,
}

impl AppWindow {
    pub fn viewport(&self) -> &ViewportWidget {
        &self.viewport
    }

    pub fn canvas(&self) -> &CanvasWidget {
        &self.canvas
    }

    fn pdflib(&self) -> Result<pdfium::Library, pdfium::Error> {
        let lib = self.pdflib.borrow().clone();
        let lib = match lib {
            Some(lib) => lib,
            None => {
                tracing::debug!("loading libpdfium");

                let res = pdfium::Library::init();
                let lib = match res {
                    Ok(lib) => lib,
                    Err(err) => {
                        tracing::error!(error=%err, "failed to load libpdfium");
                        return Err(err);
                    }
                };

                tracing::debug!("libpdfium loaded successfully");

                *self.pdflib.borrow_mut() = Some(lib.clone());
                lib
            }
        };

        Ok(lib)
    }

    pub fn open_file(&self, file: File) {
        glib::MainContext::default().spawn_local(clone!(@weak self as win => async move {
            let path = file.path().unwrap_or_default();

            tracing::info!(file=?path, "loading file");

            // load file to buffer
            let result = file.load_bytes_future().await;
            let (data, _etag) = match result {
                Ok(res) => res,
                Err(err) => {
                    tracing::warn!(file=?path, error=?err.message(), "failed to load file");

                    let toast = adw::Toast::new(&format!("{err}"));
                    toast.set_priority(adw::ToastPriority::High);
                    win.overlay.add_toast(toast);
                    return;
                },
            };

            let data = data.to_vec();

            // load pdfium library
            let pdflib = match win.pdflib() {
                Ok(pdflib) => pdflib,
                Err(_) => {
                    let dialog = gtk::AlertDialog::builder()
                        .message("Error loading pdfium")
                        .detail(
                            "Failed to load shared libraries for pdfium. \
                            Please ensure that the pdfium library is installed."
                        )
                        .build();

                    let _ = dialog.choose_future(Some(&*win.obj())).await;

                    win.obj().destroy();
                    return;
                }
            };

            // parse document
            let result = pdflib.load_buffer(data, None);
            let doc = match result {
                Ok(doc) => doc,
                Err(err) => {
                    tracing::warn!(file=?path, error=%err, "failed to parse document");

                    let toast = adw::Toast::new(&format!("Error: {err}"));
                    toast.set_priority(adw::ToastPriority::High);
                    win.overlay.add_toast(toast);
                    return;
                },
            };

            // get metadata for titlebar
            let title = doc.metadata()
                .get(pdfium::doc::MetadataTag::Title)
                .unwrap()
                .unwrap_or_else(|| "Untitled Document".into());

            let filename = path.file_name()
                .unwrap_or_default()
                .to_string_lossy();

            win.window_title.set_title(&title);
            win.window_title.set_subtitle(&filename);

            // update canvas
            win.canvas().set_document(doc);
            win.viewport().set_offset_and_scale(vector![0.0, 0.0], 1.0);
            win.viewport().fit_width();

            tracing::info!(file=?path, title, "file loaded");

            // notify user
            let toast = adw::Toast::new(&format!("File loaded: \"{}\"", filename));
            win.overlay.add_toast(toast);
        }));
    }

    pub fn close_file(&self) {
        self.canvas().clear();
        self.window_title.set_title("PDF Annotator Prototype");
        self.window_title.set_subtitle("No Document Selected");
    }
}

#[glib::object_subclass]
impl ObjectSubclass for AppWindow {
    const NAME: &'static str = "AppWindow";
    type Type = super::AppWindow;
    type ParentType = adw::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for AppWindow {
    fn constructed(&self) {
        self.parent_constructed();

        let action_doc_open = SimpleAction::new("document-open", None);
        action_doc_open.connect_activate(clone!(@weak self as win => move |_, _| {
            let filter_pdf = FileFilter::new();
            filter_pdf.add_mime_type("application/pdf");
            filter_pdf.add_suffix("pdf");
            filter_pdf.set_name(Some("PDF Documents"));

            let filter_all = FileFilter::new();
            filter_all.set_name(Some("All Files"));
            filter_all.add_mime_type("*/*");
            filter_all.add_suffix("*");

            let filters = ListStore::new(FileFilter::static_type());
            filters.append(&filter_pdf);
            filters.append(&filter_all);

            let filechooser = FileDialog::builder()
                .title("Open Document")
                .modal(true)
                .accept_label("Open")
                .filters(&filters)
                .default_filter(&filter_pdf)
                .build();

            filechooser.open(
                Some(&*win.obj()),
                None::<&gtk::gio::Cancellable>,
                clone!(@weak win => move |result| {
                    if let Ok(file) = result {
                        win.open_file(file);
                    }
                }),
            );
        }));

        let action_doc_close = SimpleAction::new("document-close", None);
        action_doc_close.connect_activate(clone!(@weak self as win => move |_, _| {
            win.close_file();
        }));

        self.obj().add_action(&action_doc_open);
        self.obj().add_action(&action_doc_close);
    }
}

impl WidgetImpl for AppWindow {}
impl WindowImpl for AppWindow {}
impl ApplicationWindowImpl for AppWindow {}
impl AdwApplicationWindowImpl for AppWindow {}
