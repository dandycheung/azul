use alloc::vec::Vec;
use core::fmt;
use std::io::Cursor;

use azul_core::resources::{RawImage, RawImageFormat};
use azul_css::{impl_result, impl_result_inner, U8Vec};
#[cfg(feature = "bmp")]
use image::codecs::bmp::BmpEncoder;
#[cfg(feature = "gif")]
use image::codecs::gif::GifEncoder;
#[cfg(feature = "hdr")]
use image::codecs::hdr::HdrEncoder;
#[cfg(feature = "jpeg")]
use image::codecs::jpeg::JpegEncoder;
#[cfg(feature = "png")]
use image::codecs::png::PngEncoder;
#[cfg(feature = "pnm")]
use image::codecs::pnm::PnmEncoder;
#[cfg(feature = "tga")]
use image::codecs::tga::TgaEncoder;
#[cfg(feature = "tiff")]
use image::codecs::tiff::TiffEncoder;
use image::error::{ImageError, LimitError, LimitErrorKind};

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub enum EncodeImageError {
    /// Crate was not compiled with the given encoder flags
    EncoderNotAvailable,
    InsufficientMemory,
    DimensionError,
    InvalidData,
    Unknown,
}

impl fmt::Display for EncodeImageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::EncodeImageError::*;
        match self {
            EncoderNotAvailable => write!(
                f,
                "Missing encoder (library was not compiled with given codec)"
            ),
            InsufficientMemory => write!(
                f,
                "Error encoding image: Not enough memory available to perform encoding operation"
            ),
            DimensionError => write!(f, "Error encoding image: Wrong dimensions"),
            InvalidData => write!(f, "Error encoding image: Invalid data format"),
            Unknown => write!(f, "Error encoding image: Unknown error"),
        }
    }
}

const fn translate_rawimage_colortype(i: RawImageFormat) -> image::ColorType {
    match i {
        RawImageFormat::R8 => image::ColorType::L8,
        RawImageFormat::RG8 => image::ColorType::La8,
        RawImageFormat::RGB8 => image::ColorType::Rgb8,
        RawImageFormat::RGBA8 => image::ColorType::Rgba8,
        RawImageFormat::BGR8 => image::ColorType::Rgb8, // TODO: ???
        RawImageFormat::BGRA8 => image::ColorType::Rgba8, // TODO: ???
        RawImageFormat::R16 => image::ColorType::L16,
        RawImageFormat::RG16 => image::ColorType::La16,
        RawImageFormat::RGB16 => image::ColorType::Rgb16,
        RawImageFormat::RGBA16 => image::ColorType::Rgba16,
        RawImageFormat::RGBF32 => image::ColorType::Rgb32F,
        RawImageFormat::RGBAF32 => image::ColorType::Rgba32F,
    }
}

fn translate_image_error_encode(i: ImageError) -> EncodeImageError {
    match i {
        ImageError::Limits(l) => match l.kind() {
            LimitErrorKind::InsufficientMemory => EncodeImageError::InsufficientMemory,
            LimitErrorKind::DimensionError => EncodeImageError::DimensionError,
            _ => EncodeImageError::Unknown,
        },
        _ => EncodeImageError::Unknown,
    }
}

impl_result!(
    U8Vec,
    EncodeImageError,
    ResultU8VecEncodeImageError,
    copy = false,
    [Debug, Clone]
);

macro_rules! encode_func {
    ($func:ident, $encoder:ident, $feature:expr) => {
        #[cfg(feature = $feature)]
        pub fn $func(image: &RawImage) -> ResultU8VecEncodeImageError {
            let mut result = Vec::<u8>::new();

            {
                let mut cursor = Cursor::new(&mut result);
                let mut encoder = $encoder::new(&mut cursor);
                let pixels = match image.pixels.get_u8_vec_ref() {
                    Some(s) => s,
                    None => {
                        return ResultU8VecEncodeImageError::Err(EncodeImageError::InvalidData);
                    }
                };

                if let Err(e) = encoder.encode(
                    pixels.as_ref(),
                    image.width as u32,
                    image.height as u32,
                    translate_rawimage_colortype(image.data_format).into(),
                ) {
                    return ResultU8VecEncodeImageError::Err(translate_image_error_encode(e));
                }
            }

            ResultU8VecEncodeImageError::Ok(result.into())
        }

        #[cfg(not(feature = $feature))]
        pub fn $func(image: &RawImage) -> ResultU8VecEncodeImageError {
            ResultU8VecEncodeImageError::Err(EncodeImageError::EncoderNotAvailable)
        }
    };
}

encode_func!(encode_bmp, BmpEncoder, "bmp");
encode_func!(encode_tga, TgaEncoder, "tga");
encode_func!(encode_tiff, TiffEncoder, "tiff");
encode_func!(encode_gif, GifEncoder, "gif");
encode_func!(encode_pnm, PnmEncoder, "pnm");

#[cfg(feature = "png")]
pub fn encode_png(image: &RawImage) -> ResultU8VecEncodeImageError {
    use image::ImageEncoder;

    let mut result = Vec::<u8>::new();

    {
        let mut cursor = Cursor::new(&mut result);
        let mut encoder = PngEncoder::new_with_quality(
            &mut cursor,
            image::codecs::png::CompressionType::Best,
            image::codecs::png::FilterType::Adaptive,
        );
        let pixels = match image.pixels.get_u8_vec_ref() {
            Some(s) => s,
            None => {
                return ResultU8VecEncodeImageError::Err(EncodeImageError::InvalidData);
            }
        };

        if let Err(e) = encoder.write_image(
            pixels.as_ref(),
            image.width as u32,
            image.height as u32,
            translate_rawimage_colortype(image.data_format).into(),
        ) {
            println!("{:?}", e);
            return ResultU8VecEncodeImageError::Err(translate_image_error_encode(e));
        }
    }

    ResultU8VecEncodeImageError::Ok(result.into())
}

#[cfg(not(feature = "png"))]
pub fn encode_png(image: &RawImage) -> ResultU8VecEncodeImageError {
    ResultU8VecEncodeImageError::Err(EncodeImageError::EncoderNotAvailable)
}

#[cfg(feature = "jpeg")]
pub fn encode_jpeg(image: &RawImage, quality: u8) -> ResultU8VecEncodeImageError {
    let mut result = Vec::<u8>::new();

    {
        let mut cursor = Cursor::new(&mut result);
        let mut encoder = JpegEncoder::new_with_quality(&mut cursor, quality);
        let pixels = match image.pixels.get_u8_vec_ref() {
            Some(s) => s,
            None => {
                return ResultU8VecEncodeImageError::Err(EncodeImageError::InvalidData);
            }
        };

        if let Err(e) = encoder.encode(
            pixels.as_ref(),
            image.width as u32,
            image.height as u32,
            translate_rawimage_colortype(image.data_format).into(),
        ) {
            println!("{:?}", e);
            return ResultU8VecEncodeImageError::Err(translate_image_error_encode(e));
        }
    }

    ResultU8VecEncodeImageError::Ok(result.into())
}

#[cfg(not(feature = "jpeg"))]
pub fn encode_jpeg(image: &RawImage, quality: u8) -> ResultU8VecEncodeImageError {
    ResultU8VecEncodeImageError::Err(EncodeImageError::EncoderNotAvailable)
}
