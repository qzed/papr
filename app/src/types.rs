use std::ops::Add;

use gtk::graphene;
use nalgebra::{Point2, Scalar, Vector2};
use num_traits::Zero;

#[derive(Debug, Clone, Copy)]
pub struct Bounds<T> {
    pub x_min: T,
    pub y_min: T,
    pub x_max: T,
    pub y_max: T,
}

impl<T: num_traits::Zero> Bounds<T> {
    pub fn zero() -> Self {
        Self {
            x_min: T::zero(),
            y_min: T::zero(),
            x_max: T::zero(),
            y_max: T::zero(),
        }
    }
}

impl From<Bounds<f64>> for graphene::Rect {
    fn from(b: Bounds<f64>) -> Self {
        graphene::Rect::new(
            b.x_min as _,
            b.y_min as _,
            (b.x_max - b.x_min) as _,
            (b.y_max - b.y_min) as _,
        )
    }
}

impl<T> Add<Vector2<T>> for Bounds<T>
where
    T: Scalar + Copy,
    T: Add<T, Output = T>,
{
    type Output = Bounds<T>;

    fn add(self, offset: Vector2<T>) -> Self::Output {
        Self {
            x_min: self.x_min + offset.x,
            y_min: self.y_min + offset.y,
            x_max: self.x_max + offset.x,
            y_max: self.y_max + offset.y,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Margin<T> {
    pub left: T,
    pub right: T,
    pub top: T,
    pub bottom: T,
}

impl<T> Margin<T> {
    pub fn zero() -> Self
    where
        T: Zero,
    {
        Self {
            left: T::zero(),
            right: T::zero(),
            top: T::zero(),
            bottom: T::zero(),
        }
    }
}

impl<T: Zero> Default for Margin<T> {
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(Debug)]
pub struct Viewport {
    pub size: Vector2<f64>,
    pub offset: Point2<f64>,
    pub scale: f64,
}
