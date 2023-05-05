use std::ops::Range;

use crate::types::Rect;

pub struct PageData<'a, F>
where
    F: Fn(&Rect<f64>) -> Rect<f64>,
{
    pub layout: &'a [Rect<f64>],
    pub visible: &'a Range<usize>,
    pub transform: &'a F,
}

impl<'a, F> PageData<'a, F>
where
    F: Fn(&Rect<f64>) -> Rect<f64>,
{
    pub fn new(layout: &'a [Rect<f64>], visible: &'a Range<usize>, transform: &'a F) -> Self {
        Self {
            layout,
            visible,
            transform,
        }
    }
}
