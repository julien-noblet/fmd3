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

    #[test]
    fn test_puzzle_reorders_tiles_at_exact_pixel_coordinates() {
        // 4x4 image split into 2x2 grid of 2x2 tiles
        let mut img = RgbaImage::new(4, 4);
        let red = Rgba([255, 0, 0, 255]);
        let green = Rgba([0, 255, 0, 255]);
        let blue = Rgba([0, 0, 255, 255]);
        let yellow = Rgba([255, 255, 0, 255]);

        for y in 0..2 {
            for x in 0..2 {
                img.put_pixel(x, y, red); // src block 0
                img.put_pixel(x + 2, y, green); // src block 1
                img.put_pixel(x, y + 2, blue); // src block 2
                img.put_pixel(x + 2, y + 2, yellow); // src block 3
            }
        }

        let mut puzzle = ImagePuzzle::new(2, 2);
        // src 0 -> dst 3, src 1 -> dst 2, src 2 -> dst 1, src 3 -> dst 0
        puzzle.set_matrix(vec![3, 2, 1, 0]);
        let out = puzzle
            .descramble_image(&DynamicImage::ImageRgba8(img))
            .unwrap()
            .to_rgba8();

        assert_eq!(*out.get_pixel(0, 0), yellow); // dst 0
        assert_eq!(*out.get_pixel(2, 0), blue); // dst 1
        assert_eq!(*out.get_pixel(0, 2), green); // dst 2
        assert_eq!(*out.get_pixel(2, 2), red); // dst 3
    }

    #[test]
    fn test_puzzle_flips_multiply_bytes_and_errors() {
        // 4x4 image, 1x1 block (whole image) to test horizontal + vertical + combined flips
        let mut img = RgbaImage::new(4, 4);
        let marker = Rgba([10, 200, 30, 255]);
        img.put_pixel(0, 0, marker);

        let mut p_h = ImagePuzzle::new(1, 1);
        p_h.set_flips(vec![1]);
        let out_h = p_h
            .descramble_image(&DynamicImage::ImageRgba8(img.clone()))
            .unwrap()
            .to_rgba8();
        assert_eq!(*out_h.get_pixel(3, 0), marker);

        let mut p_v = ImagePuzzle::new(1, 1);
        p_v.set_flips(vec![2]);
        let out_v = p_v
            .descramble_image(&DynamicImage::ImageRgba8(img.clone()))
            .unwrap()
            .to_rgba8();
        assert_eq!(*out_v.get_pixel(0, 3), marker);

        let mut p_hv = ImagePuzzle::new(1, 1);
        p_hv.set_flips(vec![3]);
        p_hv.multiply = 2;
        let mut png_in = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut png_in, ImageFormat::Png)
            .unwrap();
        let png_out = p_hv
            .descramble_bytes(&png_in.into_inner(), ImageFormat::Png)
            .unwrap();
        let decoded = image::load_from_memory(&png_out).unwrap().to_rgba8();
        assert_eq!(*decoded.get_pixel(3, 3), marker);

        // Error cases
        let mut bad_len = ImagePuzzle::new(2, 2);
        bad_len.set_matrix(vec![0, 1]);
        assert!(bad_len
            .descramble_image(&DynamicImage::ImageRgba8(decoded.clone()))
            .is_err());

        let mut bad_idx = ImagePuzzle::new(2, 2);
        bad_idx.set_matrix(vec![0, 1, 2, 9]);
        assert!(bad_idx
            .descramble_image(&DynamicImage::ImageRgba8(decoded))
            .is_err());
    }
}
