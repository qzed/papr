use gtk::graphene;
use nalgebra::{Vector2, Point2};

#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

impl From<Aabb> for graphene::Rect {
    fn from(b: Aabb) -> Self {
        graphene::Rect::new(
            b.x_min as _,
            b.y_min as _,
            (b.x_max - b.x_min) as _,
            (b.y_max - b.y_min) as _,
        )
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Margin {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Debug)]
pub struct Viewport {
    pub size: Vector2<f64>,
    pub offset: Point2<f64>,
    pub scale: f64,
}
