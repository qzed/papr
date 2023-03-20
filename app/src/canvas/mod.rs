use gtk::graphene;
use gtk::traits::SnapshotExt;
use gtk::Snapshot;

use crate::types::{Bounds, Viewport};

#[derive(Debug)]
pub struct Canvas {
    bounds: Bounds,
}

impl Canvas {
    pub fn new(bounds: Bounds) -> Self {
        Self { bounds }
    }

    pub fn bounds(&self) -> &Bounds {
        &self.bounds
    }

    pub fn render(&self, viewport: &Viewport, snapshot: &Snapshot) {
        snapshot.translate(&graphene::Point::new(
            -viewport.offset.x as f32,
            -viewport.offset.y as f32,
        ));
        snapshot.scale(viewport.scale as f32, viewport.scale as f32);

        // clip drawing to canvas area
        snapshot.push_clip(&self.bounds.into());

        // TODO

        // temporary: draw background + grid
        snapshot.append_color(
            &gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
            &graphene::Rect::from(self.bounds),
        );

        for x in (self.bounds.x_min as i32..=self.bounds.x_max as i32).step_by(25) {
            snapshot.append_color(
                &gtk::gdk::RGBA::new(0.0, 0.3, 0.6, 1.0),
                &graphene::Rect::new(
                    x as f32 - 0.5,
                    self.bounds.y_min as f32,
                    1.0,
                    (self.bounds.y_max - self.bounds.y_min) as f32,
                ),
            );
        }

        for y in (self.bounds.y_min as i32..=self.bounds.y_max as i32).step_by(25) {
            snapshot.append_color(
                &gtk::gdk::RGBA::new(0.0, 0.3, 0.6, 1.0),
                &graphene::Rect::new(
                    self.bounds.x_min as f32,
                    y as f32 - 0.5,
                    (self.bounds.x_max - self.bounds.x_min) as f32,
                    1.0,
                ),
            );
        }

        // pop the clip
        snapshot.pop();
    }
}
