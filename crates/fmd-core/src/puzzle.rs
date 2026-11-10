use crate::errors::{Error, Result};
use image::{imageops, DynamicImage, GenericImageView, ImageFormat, RgbaImage};
use std::io::Cursor;

pub struct ImagePuzzle {
    pub hor_block: u32,
    pub ver_block: u32,
    pub multiply: u32,
    pub matrix: Vec<u32>,
    pub flips: Vec<u32>,
}

impl ImagePuzzle {
    pub fn new(hor_block: u32, ver_block: u32) -> Self {
        let size = (hor_block * ver_block) as usize;
        let matrix = (0..size as u32).collect();
        let flips = vec![0; size];
        Self {
            hor_block,
            ver_block,
            multiply: 1,
            matrix,
            flips,
        }
    }

    pub fn set_matrix(&mut self, matrix: Vec<u32>) {
        self.matrix = matrix;
    }

    pub fn set_flips(&mut self, flips: Vec<u32>) {
        self.flips = flips;
    }

    pub fn descramble_image(&self, img: &DynamicImage) -> Result<DynamicImage> {
        let (width, height) = img.dimensions();
        let total_blocks = self.hor_block * self.ver_block;

        if self.matrix.len() < total_blocks as usize {
            return Err(Error::Custom(
                "Matrix size too small for puzzle dimensions".into(),
            ));
        }

        let (block_width, block_height) = if self.multiply <= 1 {
            (width / self.hor_block, height / self.ver_block)
        } else {
            (
                (width / (self.hor_block * self.multiply)) * self.multiply,
                (height / (self.ver_block * self.multiply)) * self.multiply,
            )
        };

        let mut output_img = RgbaImage::new(width, height);
        let rgba_input = img.to_rgba8();

        for i in 0..total_blocks {
            let target_idx = self.matrix[i as usize];
            if target_idx >= total_blocks {
                return Err(Error::Custom(format!(
                    "Matrix value {} out of bounds",
                    target_idx
                )));
            }

            // Destination coords
            let dst_row = target_idx / self.hor_block;
            let dst_col = target_idx % self.hor_block;
            let dx = dst_col * block_width;
            let dy = dst_row * block_height;

            // Source coords
            let src_row = i / self.hor_block;
            let src_col = i % self.hor_block;
            let sx = src_col * block_width;
            let sy = src_row * block_height;

            let tile_w = block_width
                .min(width.saturating_sub(sx))
                .min(width.saturating_sub(dx));
            let tile_h = block_height
                .min(height.saturating_sub(sy))
                .min(height.saturating_sub(dy));

            if tile_w == 0 || tile_h == 0 {
                continue;
            }

            let mut tile = imageops::crop_imm(&rgba_input, sx, sy, tile_w, tile_h).to_image();

            let flip = self.flips.get(i as usize).copied().unwrap_or(0);
            if (flip & 1) != 0 {
                tile = imageops::flip_horizontal(&tile);
            }
            if (flip & 2) != 0 {
                tile = imageops::flip_vertical(&tile);
            }

            imageops::overlay(&mut output_img, &tile, dx as i64, dy as i64);
        }

        Ok(DynamicImage::ImageRgba8(output_img))
    }

    pub fn descramble_bytes(&self, input_bytes: &[u8], format: ImageFormat) -> Result<Vec<u8>> {
        let img = image::load_from_memory(input_bytes)?;
        let descrambled = self.descramble_image(&img)?;
        let mut out = Cursor::new(Vec::new());
        descrambled.write_to(&mut out, format)?;
        Ok(out.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn test_puzzle_identity() {
        let mut puzzle = ImagePuzzle::new(2, 2);
        // Identity matrix [0, 1, 2, 3]
        puzzle.set_matrix(vec![0, 1, 2, 3]);

        let mut img = RgbaImage::new(100, 100);
        img.put_pixel(10, 10, Rgba([255, 0, 0, 255]));

        let dyn_img = DynamicImage::ImageRgba8(img);
        let res = puzzle.descramble_image(&dyn_img).unwrap();

        assert_eq!(res.width(), 100);
        assert_eq!(res.height(), 100);
    }
}
