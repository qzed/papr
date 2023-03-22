use nalgebra::{vector, Vector2};
use pdfium::doc::Page;

use crate::types::Bounds;

pub struct Layout {
    pub bounds: Bounds<f64>,
    pub offsets: Vec<Vector2<f64>>,
}

pub trait LayoutProvider {
    fn compute(&self, pages: &[Page], space: f64) -> Layout;
}

pub struct VerticalLayout;
pub struct HorizontalLayout;

impl LayoutProvider for VerticalLayout {
    fn compute(&self, pages: &[Page], space: f64) -> Layout {
        let mut bounds = Bounds::<f64>::zero();
        let mut offsets = Vec::with_capacity(pages.len());

        for page in pages {
            bounds.x_max = bounds.x_max.max(page.width() as _);
        }

        if let Some(page) = pages.first() {
            let x = (bounds.x_max - page.width() as f64) / 2.0;

            offsets.push(vector![x, 0.0]);

            bounds.y_max = page.height() as f64;
        }

        for page in pages.iter().skip(1) {
            let x = (bounds.x_max - page.width() as f64) / 2.0;
            bounds.y_max += space;

            offsets.push(vector![x, bounds.y_max]);

            bounds.y_max += page.height() as f64;
        }

        Layout { bounds, offsets }
    }
}

impl LayoutProvider for HorizontalLayout {
    fn compute(&self, pages: &[Page], space: f64) -> Layout {
        let mut bounds = Bounds::<f64>::zero();
        let mut offsets = Vec::with_capacity(pages.len());

        for page in pages {
            bounds.y_max = bounds.y_max.max(page.height() as _);
        }

        if let Some(page) = pages.first() {
            let y = (bounds.y_max - page.height() as f64) / 2.0;

            offsets.push(vector![0.0, y]);

            bounds.x_max = page.width() as f64;
        }

        for page in pages.iter().skip(1) {
            let y = (bounds.y_max - page.height() as f64) / 2.0;
            bounds.x_max += space;

            offsets.push(vector![bounds.x_max, y]);

            bounds.x_max += page.width() as f64;
        }

        Layout { bounds, offsets }
    }
}
