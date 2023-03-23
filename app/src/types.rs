use std::ops::{Add, Sub};

use gtk::graphene;
use nalgebra::{point, vector};
use nalgebra::{Point2, Scalar, Vector2};
use num_traits::{Zero, Float};

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

impl From<Bounds<f32>> for graphene::Rect {
    fn from(b: Bounds<f32>) -> Self {
        graphene::Rect::new(
            b.x_min,
            b.y_min,
            b.x_max - b.x_min,
            b.y_max - b.y_min,
        )
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
pub struct Rect<T: Scalar> {
    pub offs: Point2<T>,
    pub size: Vector2<T>,
}

impl<T: Scalar> Rect<T> {
    pub fn new(offs: Point2<T>, size: Vector2<T>) -> Self {
        Self { offs, size }
    }

    pub fn clip(&self, other: &Rect<T>) -> Self
    where
        T: Copy,
        T: Ord,
        T: Add<T, Output=T>,
        T: Sub<T, Output=T>,
    {
        let offs = point![self.offs.x.max(other.offs.x), self.offs.y.max(other.offs.y)];
        let size = vector![
            (self.offs.x + self.size.x).min(other.offs.x + other.size.x) - offs.x,
            (self.offs.y + self.size.y).min(other.offs.y + other.size.y) - offs.y
        ];

        Self { offs, size }
    }

    pub fn round(&self) -> Self
    where
        T: Float,
    {
        Self {
            offs: point![self.offs.x.round(), self.offs.y.round()],
            size: vector![self.size.x.round(), self.size.y.round()],
        }
    }
}

impl From<Rect<f32>> for graphene::Rect {
    fn from(r: Rect<f32>) -> Self {
        graphene::Rect::new(
            r.offs.x,
            r.offs.y,
            r.size.x,
            r.size.y,
        )
    }
}

impl From<Rect<f64>> for graphene::Rect {
    fn from(r: Rect<f64>) -> Self {
        graphene::Rect::new(
            r.offs.x as _,
            r.offs.y as _,
            r.size.x as _,
            r.size.y as _,
        )
    }
}

impl From<Rect<i64>> for graphene::Rect {
    fn from(r: Rect<i64>) -> Self {
        graphene::Rect::new(
            r.offs.x as _,
            r.offs.y as _,
            r.size.x as _,
            r.size.y as _,
        )
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
    pub r: Rect<f64>,
    pub scale: f64,
}
