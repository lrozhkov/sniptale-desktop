//! PNG frame geometry validation for screenshot encoding.

#[cfg(windows)]
mod windows_wic;

#[cfg(windows)]
pub(crate) fn encode_bgra_png_file(
    frame: BgraFrame<'_>,
    path: &std::path::Path,
) -> Result<(), windows_wic::WicPngError> {
    windows_wic::encode_bgra_png_file(frame, path)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BgraFrameError {
    EmptyDimensions,
    StrideTooSmall,
    GeometryTooLarge,
    BufferTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BgraFrame<'a> {
    width: u32,
    height: u32,
    stride: u32,
    pixels: &'a [u8],
}

impl<'a> BgraFrame<'a> {
    pub(crate) fn new(
        width: u32,
        height: u32,
        stride: u32,
        pixels: &'a [u8],
    ) -> Result<Self, BgraFrameError> {
        if width == 0 || height == 0 {
            return Err(BgraFrameError::EmptyDimensions);
        }
        let min_stride = width
            .checked_mul(4)
            .ok_or(BgraFrameError::GeometryTooLarge)?;
        if stride < min_stride {
            return Err(BgraFrameError::StrideTooSmall);
        }
        let required_len = usize::try_from(
            u64::from(stride)
                .checked_mul(u64::from(height))
                .ok_or(BgraFrameError::GeometryTooLarge)?,
        )
        .map_err(|_| BgraFrameError::GeometryTooLarge)?;
        if pixels.len() < required_len {
            return Err(BgraFrameError::BufferTooSmall);
        }
        Ok(Self {
            width,
            height,
            stride,
            pixels,
        })
    }

    pub(crate) const fn width(self) -> u32 {
        self.width
    }

    pub(crate) const fn height(self) -> u32 {
        self.height
    }

    pub(crate) const fn stride(self) -> u32 {
        self.stride
    }

    pub(crate) const fn pixels(self) -> &'a [u8] {
        self.pixels
    }
}
