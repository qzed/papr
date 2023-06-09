use std::ops::{Add, AddAssign, Mul, Range, Sub};

use gtk::graphene;
use itertools::{Itertools, Product};
use num_traits::{Float, Zero};
use simba::scalar::SubsetOf;

use na::{point, vector, RealField};
use na::{Point2, Scalar, Vector2};
use nalgebra as na;

#[derive(Debug, Clone, Copy)]
pub struct Bounds<T> {
    pub x_min: T,
    pub y_min: T,
    pub x_max: T,
    pub y_max: T,
}

impl<T> Bounds<T> {
    #[inline]
    pub fn zero() -> Self
    where
        T: Zero,
    {
        Self {
            x_min: T::zero(),
            y_min: T::zero(),
            x_max: T::zero(),
            y_max: T::zero(),
        }
    }

    #[inline]
    pub fn rect(&self) -> Rect<T>
    where
        T: Copy,
        T: Scalar,
        T: Sub<T, Output = T>,
    {
        Rect {
            offs: point![self.x_min, self.y_min],
            size: vector![self.x_max - self.x_min, self.y_max - self.y_min],
        }
    }

    #[inline]
    pub fn range_x(&self) -> Range<T>
    where
        T: Copy,
    {
        (self.x_min)..(self.x_max)
    }

    #[inline]
    pub fn range_y(&self) -> Range<T>
    where
        T: Copy,
    {
        (self.y_min)..(self.y_max)
    }

    pub fn range_iter(&self) -> Product<Range<T>, Range<T>>
    where
        T: Copy,
        Range<T>: Iterator<Item = T>,
    {
        self.range_x().cartesian_product(self.range_y())
    }

    #[inline]
    pub fn clip(&self, other: &Bounds<T>) -> Self
    where
        T: Scalar,
        T: Copy,
        T: PartialOrd,
        T: Add<T, Output = T>,
        T: Sub<T, Output = T>,
    {
        fn min<T>(a: T, b: T) -> T
        where
            T: Copy,
            T: PartialOrd,
            T: Add<T, Output = T>,
            T: Sub<T, Output = T>,
        {
            if a < b {
                a
            } else {
                b
            }
        }

        fn max<T>(a: T, b: T) -> T
        where
            T: Copy,
            T: PartialOrd,
            T: Add<T, Output = T>,
            T: Sub<T, Output = T>,
        {
            if a > b {
                a
            } else {
                b
            }
        }

        Bounds {
            x_min: max(self.x_min, other.x_min),
            y_min: max(self.y_min, other.y_min),
            x_max: min(self.x_max, other.x_max),
            y_max: min(self.y_max, other.y_max),
        }
    }

    #[inline]
    pub fn intersects(&self, other: &Bounds<T>) -> bool
    where
        T: PartialOrd,
    {
        self.x_min < other.x_max
            && self.x_max > other.x_min
            && self.y_min < other.y_max
            && self.y_max > other.y_min
    }

    #[inline]
    pub fn contains(&self, other: &Bounds<T>) -> bool
    where
        T: PartialOrd,
    {
        self.x_min >= other.x_max
            && self.x_max <= other.x_min
            && self.y_min >= other.y_max
            && self.y_max <= other.y_min
    }

    #[inline]
    pub fn contains_point(&self, point: &Point2<T>) -> bool
    where
        T: Scalar,
        T: PartialOrd,
    {
        self.x_min <= point.x
            && self.x_max > point.x
            && self.y_min <= point.y
            && self.y_max > point.y
    }

    #[inline]
    pub fn translate(&self, offset: &Vector2<T>) -> Self
    where
        T: Scalar + Copy,
        T: Add<T, Output = T>,
    {
        Self {
            x_min: self.x_min + offset.x,
            x_max: self.x_max + offset.x,
            y_min: self.y_min + offset.y,
            y_max: self.y_max + offset.y,
        }
    }

    #[inline]
    pub fn scale(&self, scale: T) -> Self
    where
        T: Scalar + Copy,
        T: Mul<T, Output = T>,
    {
        Self {
            x_min: self.x_min * scale,
            x_max: self.x_max * scale,
            y_min: self.y_min * scale,
            y_max: self.y_max * scale,
        }
    }

    #[inline]
    pub fn cast<U>(&self) -> Bounds<U>
    where
        T: Scalar + Copy,
        U: Scalar,
        T: SubsetOf<U>,
    {
        Bounds {
            x_min: na::convert(self.x_min),
            y_min: na::convert(self.y_min),
            x_max: na::convert(self.x_max),
            y_max: na::convert(self.y_max),
        }
    }

    #[inline]
    pub fn cast_unchecked<U>(&self) -> Bounds<U>
    where
        T: Copy,
        U: Scalar,
        U: SubsetOf<T>,
    {
        Bounds {
            x_min: na::convert_unchecked(self.x_min),
            y_min: na::convert_unchecked(self.y_min),
            x_max: na::convert_unchecked(self.x_max),
            y_max: na::convert_unchecked(self.y_max),
        }
    }

    #[inline]
    pub fn round_outwards(&self) -> Self
    where
        T: Copy,
        T: RealField,
    {
        Self {
            x_min: self.x_min.floor(),
            y_min: self.y_min.floor(),
            x_max: self.x_max.ceil(),
            y_max: self.y_max.ceil(),
        }
    }
}

impl Bounds<i64> {
    #[inline]
    pub fn tiled(&self, tile_size: &Vector2<i64>) -> Self {
        Self {
            x_min: self.x_min / tile_size.x,
            y_min: self.y_min / tile_size.y,
            x_max: (self.x_max + tile_size.x - 1) / tile_size.x,
            y_max: (self.y_max + tile_size.y - 1) / tile_size.y,
        }
    }
}

impl<T> From<Rect<T>> for Bounds<T>
where
    T: Copy,
    T: Scalar,
    T: Add<T, Output = T>,
{
    fn from(r: Rect<T>) -> Self {
        r.bounds()
    }
}

impl From<Bounds<f32>> for graphene::Rect {
    fn from(b: Bounds<f32>) -> Self {
        graphene::Rect::new(b.x_min, b.y_min, b.x_max - b.x_min, b.y_max - b.y_min)
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

#[derive(Debug, Clone, Copy)]
pub struct Rect<T: Scalar> {
    pub offs: Point2<T>,
    pub size: Vector2<T>,
}

impl<T: Scalar> Rect<T> {
    #[inline]
    pub fn new(offs: Point2<T>, size: Vector2<T>) -> Self {
        Self { offs, size }
    }

    #[inline]
    pub fn clip(&self, other: &Rect<T>) -> Self
    where
        T: Copy,
        T: PartialOrd,
        T: Add<T, Output = T>,
        T: Sub<T, Output = T>,
    {
        self.bounds().clip(&other.bounds()).rect()
    }

    #[inline]
    pub fn intersects(&self, other: &Rect<T>) -> bool
    where
        T: Copy,
        T: PartialOrd,
        T: Add<T, Output = T>,
        T: Sub<T, Output = T>,
    {
        self.bounds().intersects(&other.bounds())
    }

    #[inline]
    pub fn contains(&self, other: &Rect<T>) -> bool
    where
        T: Copy,
        T: PartialOrd,
        T: Add<T, Output = T>,
        T: Sub<T, Output = T>,
    {
        self.bounds().contains(&other.bounds())
    }

    #[inline]
    pub fn contains_point(&self, point: &Point2<T>) -> bool
    where
        T: Copy,
        T: PartialOrd,
        T: Add<T, Output = T>,
        T: Sub<T, Output = T>,
    {
        self.bounds().contains_point(point)
    }

    #[inline]
    pub fn round(&self) -> Self
    where
        T: Float,
    {
        Self {
            offs: point![self.offs.x.round(), self.offs.y.round()],
            size: vector![self.size.x.round(), self.size.y.round()],
        }
    }

    #[inline]
    pub fn bounds(&self) -> Bounds<T>
    where
        T: Copy,
        T: Add<T, Output = T>,
    {
        Bounds {
            x_min: self.offs.x,
            y_min: self.offs.y,
            x_max: self.offs.x + self.size.x,
            y_max: self.offs.y + self.size.y,
        }
    }

    #[inline]
    pub fn range_x(&self) -> Range<T>
    where
        T: Copy,
        T: Add<T, Output = T>,
    {
        (self.offs.x)..(self.offs.y + self.size.y)
    }

    #[inline]
    pub fn range_y(&self) -> Range<T>
    where
        T: Copy,
        T: Add<T, Output = T>,
    {
        (self.offs.y)..(self.offs.y + self.size.y)
    }

    #[inline]
    pub fn translate(&self, offset: &Vector2<T>) -> Self
    where
        T: Copy,
        T: Add<T, Output = T>,
        T: AddAssign,
    {
        Self {
            offs: self.offs + offset,
            size: self.size,
        }
    }

    #[inline]
    pub fn scale(&self, scale: T) -> Self
    where
        T: Copy,
        T: Mul<T, Output = T>,
    {
        Self {
            offs: point![self.offs.x * scale, self.offs.y * scale],
            size: vector![self.size.x * scale, self.size.y * scale],
        }
    }

    #[inline]
    pub fn cast<U>(&self) -> Rect<U>
    where
        T: Copy,
        U: Scalar,
        T: SubsetOf<U>,
    {
        Rect {
            offs: na::convert(self.offs),
            size: na::convert(self.size),
        }
    }

    #[inline]
    pub fn cast_unchecked<U>(&self) -> Rect<U>
    where
        T: Copy,
        U: Scalar,
        U: SubsetOf<T>,
    {
        Rect {
            offs: na::convert_unchecked(self.offs),
            size: na::convert_unchecked(self.size),
        }
    }
}

impl<T> From<Bounds<T>> for Rect<T>
where
    T: Copy,
    T: Scalar,
    T: Sub<T, Output = T>,
{
    fn from(b: Bounds<T>) -> Self {
        b.rect()
    }
}

impl From<Rect<f32>> for graphene::Rect {
    fn from(r: Rect<f32>) -> Self {
        graphene::Rect::new(r.offs.x, r.offs.y, r.size.x, r.size.y)
    }
}

impl From<Rect<f64>> for graphene::Rect {
    fn from(r: Rect<f64>) -> Self {
        graphene::Rect::new(r.offs.x as _, r.offs.y as _, r.size.x as _, r.size.y as _)
    }
}

impl From<Rect<i64>> for graphene::Rect {
    fn from(r: Rect<i64>) -> Self {
        graphene::Rect::new(r.offs.x as _, r.offs.y as _, r.size.x as _, r.size.y as _)
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
