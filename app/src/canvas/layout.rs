use nalgebra::{point, vector};

use crate::types::{Bounds, Rect};

pub struct Layout {
    pub bounds: Bounds<f64>,
    pub rects: Vec<Rect<f64>>,
}

pub trait LayoutProvider {
    fn compute(&self, page_sizes: impl IntoIterator<Item = (f64, f64)>, space: f64) -> Layout;
}

pub struct VerticalLayout;
pub struct HorizontalLayout;

impl LayoutProvider for VerticalLayout {
    fn compute(&self, page_sizes: impl IntoIterator<Item = (f64, f64)>, space: f64) -> Layout {
        let mut rects: Vec<Rect<f64>> = page_sizes
            .into_iter()
            .map(|(w, h)| Rect::new(point![0.0, 0.0], vector![w, h]))
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
    fn compute(&self, page_sizes: impl IntoIterator<Item = (f64, f64)>, space: f64) -> Layout {
        let mut rects: Vec<Rect<f64>> = page_sizes
            .into_iter()
            .map(|(w, h)| Rect::new(point![0.0, 0.0], vector![w, h]))
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
