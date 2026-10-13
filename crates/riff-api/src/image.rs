use gdk_pixbuf::prelude::*;

/// Largest playlist cover accepted, counted in base64.
const COVER_MAX_BASE64_BYTES: usize = 256 * 1024;
const COVER_SIZE: i32 = 640;

pub fn encode_cover(data: &[u8]) -> Option<String> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.write(data).ok()?;
    loader.close().ok()?;
    let pixbuf = loader.pixbuf()?;
    let pixbuf = pixbuf.apply_embedded_orientation().unwrap_or(pixbuf);

    let (width, height) = (pixbuf.width(), pixbuf.height());
    let side = width.min(height);
    let square = pixbuf.new_subpixbuf((width - side) / 2, (height - side) / 2, side, side);
    let size = side.min(COVER_SIZE);
    let scaled = square.scale_simple(size, size, gdk_pixbuf::InterpType::Bilinear)?;
    let cover = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, size, size)?;
    cover.fill(0xffffffff);
    scaled.composite(
        &cover,
        0,
        0,
        size,
        size,
        0.0,
        0.0,
        1.0,
        1.0,
        gdk_pixbuf::InterpType::Nearest,
        255,
    );

    ["90", "75", "60", "45", "30"]
        .into_iter()
        .find_map(|quality| {
            let jpeg = cover
                .save_to_bufferv("jpeg", &[("quality", quality)])
                .ok()?;
            let encoded = glib::base64_encode(&jpeg);
            (encoded.len() <= COVER_MAX_BASE64_BYTES).then(|| encoded.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: i32, height: i32, alpha: bool) -> Vec<u8> {
        let pixbuf =
            gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, alpha, 8, width, height).unwrap();
        pixbuf.fill(0x3584e4ff);
        pixbuf.save_to_bufferv("png", &[]).unwrap()
    }

    #[test]
    fn cover_is_a_square_jpeg_within_the_limit() {
        for alpha in [false, true] {
            let encoded = encode_cover(&png(1500, 900, alpha)).expect("cover should encode");
            assert!(encoded.len() <= COVER_MAX_BASE64_BYTES);

            let jpeg = glib::base64_decode(&encoded);
            assert_eq!(&jpeg[..3], [0xff, 0xd8, 0xff]);
            let loader = gdk_pixbuf::PixbufLoader::new();
            loader.write(&jpeg).unwrap();
            loader.close().unwrap();
            let cover = loader.pixbuf().unwrap();
            assert_eq!((cover.width(), cover.height()), (COVER_SIZE, COVER_SIZE));
            let pixel = &cover.read_pixel_bytes()[..3];
            assert!(pixel[2] > pixel[0] && pixel[2] > 0xc0, "{pixel:?}");
        }
    }

    #[test]
    fn cover_rejects_data_that_is_not_an_image() {
        assert!(encode_cover(b"not an image").is_none());
    }
}
