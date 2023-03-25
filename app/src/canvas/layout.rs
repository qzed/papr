use na::point;
use nalgebra as na;

use pdfium::doc::Page;

use crate::types::{Bounds, Rect};

pub struct Layout {
    pub bounds: Bounds<f64>,
    pub rects: Vec<Rect<f64>>,
}

pub trait LayoutProvider {
    fn compute<'a>(&self, pages: impl IntoIterator<Item = &'a Page>, space: f64) -> Layout;
}

pub struct VerticalLayout;
pub struct HorizontalLayout;

impl LayoutProvider for VerticalLayout {
    fn compute<'a>(&self, pages: impl IntoIterator<Item = &'a Page>, space: f64) -> Layout {
        let mut rects: Vec<Rect<f64>> = pages
            .into_iter()
            .map(|p| Rect::new(point![0.0, 0.0], na::convert(p.size())))
            .collect();

        let mut bounds = Bounds::zero();
        bounds.x_max = rects
            .iter()
            .fold(0.0, |x: f64, r: &Rect<f64>| x.max(r.size.x));

        if let Some(r) = rects.first_mut() {
            let x = (bounds.x_max - r.size.x) / 2.0;

            r.offs = point![x, bounds.y_max];
            bounds.y_max += r.size.y;
        }

        for r in rects.iter_mut().skip(1) {
            let x = (bounds.x_max - r.size.x) / 2.0;

            bounds.y_max += space;
            r.offs = point![x, bounds.y_max];
            bounds.y_max += r.size.y;
        }

        Layout { bounds, rects }
    }
}

impl LayoutProvider for HorizontalLayout {
    fn compute<'a>(&self, pages: impl IntoIterator<Item = &'a Page>, space: f64) -> Layout {
        let mut rects: Vec<Rect<f64>> = pages
            .into_iter()
            .map(|p| Rect::new(point![0.0, 0.0], na::convert(p.size())))
            .collect();

        let mut bounds = Bounds::zero();
        bounds.y_max = rects
            .iter()
            .fold(0.0, |y: f64, r: &Rect<f64>| y.max(r.size.y));

        if let Some(r) = rects.first_mut() {
            let y = (bounds.y_max - r.size.y) / 2.0;

            r.offs = point![bounds.x_max, y];
            bounds.x_max += r.size.x;
        }

        for r in rects.iter_mut().skip(1) {
            let y = (bounds.y_max - r.size.y) / 2.0;

            bounds.x_max += space;
            r.offs = point![bounds.x_max, y];
            bounds.x_max += r.size.x;
        }

        Layout { bounds, rects }
    }
}
