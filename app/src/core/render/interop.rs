use nalgebra::Vector2;

pub struct Bitmap {
    pub buffer: Box<[u8]>,
    pub size: Vector2<u32>,
    pub stride: u32,
}

pub trait TileFactory {
    type Data;

    fn create(&self, bmp: Bitmap) -> Self::Data;
}
