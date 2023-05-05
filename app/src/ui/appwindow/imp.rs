use std::cell::RefCell;

use adw::subclass::prelude::AdwApplicationWindowImpl;
use gtk::gio::{File, SimpleAction};
use gtk::glib::clone;
use gtk::glib::subclass::InitializingObject;
use gtk::prelude::{ActionMapExt, FileExt};
use gtk::subclass::prelude::{
    ApplicationWindowImpl, CompositeTemplateClass, CompositeTemplateInitializingExt, ObjectImpl,
    ObjectImplExt, ObjectSubclass, ObjectSubclassExt, WidgetImpl, WindowImpl,
};
use gtk::subclass::widget::WidgetClassSubclassExt;
use gtk::traits::{FileChooserExt, NativeDialogExt};
use gtk::{
    glib, CompositeTemplate, FileChooserAction, FileChooserNative, FileFilter, ResponseType,
    TemplateChild,
};
use nalgebra::vector;

use crate::ui::canvas::CanvasWidget;
use crate::ui::viewport::ViewportWidget;

#[derive(CompositeTemplate, Default)]
#[template(resource = "/io/mxnluz/papr/ui/appwindow.ui")]
pub struct AppWindow {
    #[template_child]
    viewport: TemplateChild<ViewportWidget>,

    #[template_child]
    canvas: TemplateChild<CanvasWidget>,

    #[template_child]
    window_title: TemplateChild<adw::WindowTitle>,

    pdflib: RefCell<Option<pdfium::Library>>,
    filechooser: RefCell<Option<FileChooserNative>>,
}

impl AppWindow {
    pub fn viewport(&self) -> &ViewportWidget {
        &self.viewport
    }

    pub fn canvas(&self) -> &CanvasWidget {
        &self.canvas
    }

    pub fn open_file(&self, file: File) {
        glib::MainContext::default().spawn_local(clone!(@weak self as win => async move {
            println!("loading file: {:?}", file.path().unwrap());

            // TODO: handle errors
            let (data, _etag) = file.load_bytes_future().await.expect("failed to load file");
            let data = data.to_vec();

            let mut pdflib = win.pdflib.borrow_mut();
            let pdflib = match pdflib.as_ref() {
                Some(pdflib) => pdflib,
                None => {
                    *pdflib = Some(pdfium::Library::init().unwrap());
                    pdflib.as_ref().unwrap()
                }
            };

            let doc = pdflib.load_buffer(data, None).unwrap();

            println!("file loaded");

            let title = doc.metadata()
                .get(pdfium::doc::MetadataTag::Title)
                .unwrap()
                .unwrap_or_else(|| "Untitled Document".into());

            let path = file.path().unwrap();
            let subtitle = path.file_name().unwrap().to_string_lossy();

            win.window_title.set_title(&title);
            win.window_title.set_subtitle(&subtitle);

            win.canvas().set_document(doc);
            win.viewport().set_offset_and_scale(vector![0.0, 0.0], 1.0);
            win.viewport().fit_width();
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
            let filter = FileFilter::new();
            filter.add_mime_type("application/pdf");
            filter.add_suffix("pdf");
            filter.set_name(Some(".pdf"));

            let filechooser = FileChooserNative::builder()
                .title("Open Document")
                .modal(true)
                .transient_for(&*win.obj())
                .accept_label("Open")
                .cancel_label("Cancel")
                .action(FileChooserAction::Open)
                .select_multiple(false)
                .filter(&filter)
                .build();

            filechooser.connect_response(clone!(@weak win => move |filechooser, rsptype| {
                match rsptype {
                    ResponseType::Accept => {
                        if let Some(file) = filechooser.file() {
                            win.open_file(file);

                            // drop the filechooser
                            *win.filechooser.borrow_mut() = None;
                        }
                    },
                    _ => {}
                }
            }));

            filechooser.show();

            // need to store filechooser as GTK doesn't keep it around by itself
            *win.filechooser.borrow_mut() = Some(filechooser);
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
