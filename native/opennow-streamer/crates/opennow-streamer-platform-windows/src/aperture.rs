#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VideoAperture {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl VideoAperture {
    pub(crate) fn new(
        media_width: u32,
        media_height: u32,
        area: Option<(i32, i32, i32, i32)>,
    ) -> Result<Self, String> {
        let aperture = match area {
            None | Some((0, 0, 0, 0)) => Self {
                x: 0,
                y: 0,
                width: media_width,
                height: media_height,
            },
            Some((x, y, width, height)) if x >= 0 && y >= 0 && width > 0 && height > 0 => Self {
                x: x as u32,
                y: y as u32,
                width: width as u32,
                height: height as u32,
            },
            Some(_) => return Err("invalid decoder display aperture".to_owned()),
        };
        aperture.validate_extent(media_width, media_height)?;
        Ok(aperture)
    }

    pub(crate) fn validate_extent(self, width: u32, height: u32) -> Result<(), String> {
        if width == 0
            || height == 0
            || width > i32::MAX as u32
            || height > i32::MAX as u32
            || self.width == 0
            || self.height == 0
            || self
                .x
                .checked_add(self.width)
                .is_none_or(|right| right > width)
            || self
                .y
                .checked_add(self.height)
                .is_none_or(|bottom| bottom > height)
        {
            return Err(format!(
                "decoder display aperture {self:?} exceeds frame extent {width}x{height}"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_allocation_does_not_change_visible_extent() {
        let aperture = VideoAperture::new(1920, 1080, None).unwrap();
        for _ in 0..120 {
            aperture.validate_extent(1920, 1088).unwrap();
            assert_eq!((aperture.width, aperture.height), (1920, 1080));
        }
    }

    #[test]
    fn explicit_aperture_preserves_offsets_and_excludes_padding() {
        let aperture = VideoAperture::new(1920, 1088, Some((8, 4, 1904, 1080))).unwrap();
        assert_eq!((aperture.x, aperture.y), (8, 4));
        assert_eq!(
            (aperture.x + aperture.width, aperture.y + aperture.height),
            (1912, 1084)
        );
        aperture.validate_extent(1920, 1088).unwrap();
        assert!(aperture.validate_extent(1920, 1080).is_err());
    }

    #[test]
    fn absent_and_default_apertures_use_media_not_allocation_extent() {
        assert_eq!(
            VideoAperture::new(1920, 1080, None),
            VideoAperture::new(1920, 1080, Some((0, 0, 0, 0)))
        );
    }

    #[test]
    fn malformed_apertures_are_rejected_without_clamping() {
        for area in [
            (-1, 0, 1920, 1080),
            (0, -1, 1920, 1080),
            (0, 0, -1, 1080),
            (0, 0, 1920, 0),
            (1, 0, 1920, 1080),
            (0, 9, 1920, 1080),
            (1, 0, 0, 0),
        ] {
            assert!(
                VideoAperture::new(1920, 1088, Some(area)).is_err(),
                "{area:?}"
            );
        }
        assert!(VideoAperture::new(0, 1080, None).is_err());
        assert!(VideoAperture::new(u32::MAX, 1080, None).is_err());
    }
}
