//! Windows Imaging Component PNG encoder adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{os::windows::ffi::OsStrExt, path::Path, ptr::null};

use windows::{
    Win32::{
        Foundation::{GENERIC_WRITE, S_FALSE},
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
            IWICBitmapEncoder, IWICBitmapFrameEncode, IWICImagingFactory, IWICStream,
            WICBitmapEncoderNoCache,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
    },
    core::PCWSTR,
};

use super::BgraFrame;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WicPngError {
    PathContainsNul,
    MissingFrame,
    UnsupportedPixelFormat,
    Windows(i32),
}

pub(crate) fn encode_bgra_png_file(frame: BgraFrame<'_>, path: &Path) -> Result<(), WicPngError> {
    let filename = wide_path(path)?;
    let _apartment = ComApartment::initialize()?;
    let factory = create_factory()?;
    let stream = create_output_stream(&factory, &filename)?;
    let encoder = create_png_encoder(&factory, &stream)?;
    let bitmap_frame = create_frame(&encoder)?;
    write_frame(&bitmap_frame, frame)?;
    commit_encoder(&encoder)?;
    Ok(())
}

#[derive(Debug)]
struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, WicPngError> {
        // SAFETY: initializes COM for the current thread only. The RAII guard calls
        // CoUninitialize exactly once after a successful initialization result.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == S_FALSE {
            Ok(Self)
        } else {
            result.ok().map(|()| Self).map_err(to_wic_error)
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: this guard is only constructed after CoInitializeEx returned success for the
        // current thread, matching COM apartment lifetime rules.
        unsafe { CoUninitialize() };
    }
}

fn create_factory() -> Result<IWICImagingFactory, WicPngError> {
    // SAFETY: creates an in-process WIC factory COM object after COM has been initialized for the
    // current thread. No raw interface pointer leaves this adapter.
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
        .map_err(to_wic_error)
}

fn create_output_stream(
    factory: &IWICImagingFactory,
    filename: &[u16],
) -> Result<IWICStream, WicPngError> {
    // SAFETY: the factory is a live COM object and returns a stream interface owned by RAII.
    let stream = unsafe { factory.CreateStream() }.map_err(to_wic_error)?;
    // SAFETY: filename is NUL-terminated and lives through the call; GENERIC_WRITE opens the WIC
    // stream for encoder output only.
    unsafe { stream.InitializeFromFilename(PCWSTR(filename.as_ptr()), GENERIC_WRITE.0) }
        .map_err(to_wic_error)?;
    Ok(stream)
}

fn create_png_encoder(
    factory: &IWICImagingFactory,
    stream: &IWICStream,
) -> Result<IWICBitmapEncoder, WicPngError> {
    // SAFETY: the factory is live and receives valid WIC container GUID pointers. The vendor GUID
    // is null to select the platform encoder.
    let encoder =
        unsafe { factory.CreateEncoder(&GUID_ContainerFormatPng, null()) }.map_err(to_wic_error)?;
    // SAFETY: encoder and stream are live COM objects owned in this function scope; WIC keeps the
    // stream for the encoder lifetime and both remain alive until commit returns.
    unsafe { encoder.Initialize(stream, WICBitmapEncoderNoCache) }.map_err(to_wic_error)?;
    Ok(encoder)
}

fn create_frame(encoder: &IWICBitmapEncoder) -> Result<IWICBitmapFrameEncode, WicPngError> {
    let mut frame = None;
    // SAFETY: encoder is live; output pointers are initialized by WIC during the call.
    unsafe { encoder.CreateNewFrame(&mut frame, std::ptr::null_mut()) }.map_err(to_wic_error)?;
    let frame = frame.ok_or(WicPngError::MissingFrame)?;
    // SAFETY: the frame is newly created and accepts default encoder options.
    unsafe { frame.Initialize(None) }.map_err(to_wic_error)?;
    Ok(frame)
}

fn write_frame(
    frame_encoder: &IWICBitmapFrameEncode,
    frame: BgraFrame<'_>,
) -> Result<(), WicPngError> {
    // SAFETY: the frame encoder is live; width and height come from a validated BgraFrame.
    unsafe { frame_encoder.SetSize(frame.width(), frame.height()) }.map_err(to_wic_error)?;
    let mut pixel_format = GUID_WICPixelFormat32bppBGRA;
    // SAFETY: WIC writes the accepted pixel format to this stack GUID; the pointer is valid for
    // the duration of the call.
    unsafe { frame_encoder.SetPixelFormat(&raw mut pixel_format) }.map_err(to_wic_error)?;
    if pixel_format != GUID_WICPixelFormat32bppBGRA {
        return Err(WicPngError::UnsupportedPixelFormat);
    }
    // SAFETY: BgraFrame validation proves stride >= width * 4 and pixels contains at least
    // stride * height bytes; WIC reads the slice only during this call.
    unsafe { frame_encoder.WritePixels(frame.height(), frame.stride(), frame.pixels()) }
        .map_err(to_wic_error)?;
    // SAFETY: commits the live frame after all required size/format/pixel calls completed.
    unsafe { frame_encoder.Commit() }.map_err(to_wic_error)
}

fn commit_encoder(encoder: &IWICBitmapEncoder) -> Result<(), WicPngError> {
    // SAFETY: commits the live encoder after its frame has been committed.
    unsafe { encoder.Commit() }.map_err(to_wic_error)
}

fn wide_path(path: &Path) -> Result<Vec<u16>, WicPngError> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    if wide[..wide.len().saturating_sub(1)].contains(&0) {
        Err(WicPngError::PathContainsNul)
    } else {
        Ok(wide)
    }
}

fn to_wic_error(error: windows::core::Error) -> WicPngError {
    WicPngError::Windows(error.code().0)
}
