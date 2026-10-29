//! D3D11 and WinRT readback for WGC frames.
#![allow(unsafe_code)]

mod recording;

use super::super::super::screenshot::{ScreenshotCaptureError, validate_raw_frame_bounds};
use super::super::recording_owner::{
    FRAME_POLL_FAILURE_LIMIT, FramePollRecovery, is_empty_frame_poll_hresult,
};
pub(super) use recording::RecordingFrameReadError;
use std::{ptr, thread, time::Duration};
use windows::{
    Graphics::{
        Capture::{
            Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind,
            GraphicsCaptureItem, GraphicsCaptureSession,
        },
        DirectX::{
            Direct3D11::{IDirect3DDevice, IDirect3DSurface},
            DirectXPixelFormat,
        },
        SizeInt32,
    },
    Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus,
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0},
            Direct3D11::{
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11Resource,
                ID3D11Texture2D,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIDevice,
            },
        },
        System::WinRT::Direct3D11::{
            CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
        },
    },
    core::Interface,
};
const FRAME_WAIT_INTERVAL: Duration = Duration::from_millis(25);

fn wait_for_recording_frame_retry() {
    thread::sleep(FRAME_WAIT_INTERVAL);
}

fn texture_from_surface(surface: &IDirect3DSurface) -> Result<ID3D11Texture2D, i32> {
    let access: IDirect3DDxgiInterfaceAccess = surface.cast().map_err(|error| error.code().0)?;
    // SAFETY: The WGC frame surface uses the documented WinRT/DXGI texture interop path.
    unsafe { access.GetInterface() }.map_err(|error| error.code().0)
}
#[derive(Debug)]
pub(super) struct D3dDevice {
    d3d: ID3D11Device,
    direct3d: IDirect3DDevice,
}
impl D3dDevice {
    pub(super) fn new() -> Result<Self, ScreenshotCaptureError> {
        let mut d3d = None;
        // SAFETY: Output pointers are valid for the duration of the call; no adapter/software
        // module is supplied, so D3D creates a hardware device with BGRA support.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(ptr::addr_of_mut!(d3d)),
                None,
                None,
            )
        }
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let d3d = d3d.ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let dxgi: IDXGIDevice = d3d
            .cast()
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        // SAFETY: dxgi is a valid IDXGIDevice derived from the D3D11 device above.
        let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let direct3d = inspectable
            .cast()
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        Ok(Self { d3d, direct3d })
    }
}
#[derive(Debug)]
pub(super) struct ReadbackFrame {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) stride: u32,
    pub(super) bgra_pixels: Vec<u8>,
    pub(super) system_relative_time_100ns: i64,
}
pub(super) fn capture_item_frame(
    device: &D3dDevice,
    item: &GraphicsCaptureItem,
    include_cursor: bool,
) -> Result<ReadbackFrame, ScreenshotCaptureError> {
    let mut session = CaptureSession::start(device, item, 1, false, include_cursor)?;
    session.next_frame(device)
}
pub(super) fn request_borderless_access() -> bool {
    GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
        .and_then(|request| request.join())
        .is_ok_and(|status| status == AppCapabilityAccessStatus::Allowed)
}
#[derive(Debug)]
pub(super) struct CaptureSession {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    poll_recovery: FramePollRecovery,
}
impl CaptureSession {
    pub(super) fn start(
        device: &D3dDevice,
        item: &GraphicsCaptureItem,
        buffer_count: i32,
        borderless_requested: bool,
        include_cursor: bool,
    ) -> Result<Self, ScreenshotCaptureError> {
        let size = item
            .Size()
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &device.direct3d,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            buffer_count,
            SizeInt32 {
                Width: size.Width,
                Height: size.Height,
            },
        )
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let session = pool
            .CreateCaptureSession(item)
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let capture_session = Self {
            pool,
            session,
            poll_recovery: FramePollRecovery::default(),
        };
        capture_session
            .session
            .SetIsCursorCaptureEnabled(include_cursor)
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        if borderless_requested {
            capture_session
                .session
                .SetIsBorderRequired(false)
                .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
            if capture_session
                .session
                .IsBorderRequired()
                .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?
            {
                return Err(ScreenshotCaptureError::CaptureUnavailable);
            }
        }
        capture_session
            .session
            .StartCapture()
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        Ok(capture_session)
    }

    pub(super) fn next_frame(
        &mut self,
        device: &D3dDevice,
    ) -> Result<ReadbackFrame, ScreenshotCaptureError> {
        for _ in 0..FRAME_POLL_FAILURE_LIMIT {
            if let Some(frame) = self
                .try_next_recording_frame(device)
                .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?
            {
                return Ok(frame);
            }
            thread::sleep(FRAME_WAIT_INTERVAL);
        }
        Err(ScreenshotCaptureError::CaptureUnavailable)
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

fn readback_texture(
    device: &D3dDevice,
    texture: &ID3D11Texture2D,
) -> Result<ReadbackFrame, ScreenshotCaptureError> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: desc is a valid out-parameter and texture is a live D3D11 texture.
    unsafe { texture.GetDesc(ptr::addr_of_mut!(desc)) };
    if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    let min_stride = desc
        .Width
        .checked_mul(4)
        .ok_or(ScreenshotCaptureError::RawFrameTooLarge)?;
    validate_raw_frame_bounds(desc.Width, desc.Height, min_stride)?;
    let staging_desc = D3D11_TEXTURE2D_DESC {
        MipLevels: 1,
        ArraySize: 1,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
        ..desc
    };
    let mut staging = None;
    // SAFETY: staging_desc describes a CPU-readable texture; staging receives the COM object.
    unsafe {
        device.d3d.CreateTexture2D(
            ptr::addr_of!(staging_desc),
            None,
            Some(ptr::addr_of_mut!(staging)),
        )
    }
    .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let staging = staging.ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
    // SAFETY: device.d3d is a live D3D11 device; the immediate context is owned by D3D.
    let context = unsafe { device.d3d.GetImmediateContext() }
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let source: ID3D11Resource = texture
        .cast()
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let target: ID3D11Resource = staging
        .cast()
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    // SAFETY: source and target are valid D3D11 resources with identical dimensions/format.
    unsafe { context.CopyResource(&target, &source) };

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    // SAFETY: target is a staging resource with CPU read access; mapped is a valid out-parameter.
    unsafe {
        context.Map(
            &target,
            0,
            D3D11_MAP_READ,
            0,
            Some(ptr::addr_of_mut!(mapped)),
        )
    }
    .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let result = copy_mapped_frame(desc.Width, desc.Height, mapped);
    // SAFETY: target was successfully mapped above and must be unmapped exactly once.
    unsafe { context.Unmap(&target, 0) };
    result
}

fn copy_mapped_frame(
    width: u32,
    height: u32,
    mapped: D3D11_MAPPED_SUBRESOURCE,
) -> Result<ReadbackFrame, ScreenshotCaptureError> {
    let stride = mapped.RowPitch;
    validate_raw_frame_bounds(width, height, stride)?;
    let byte_len = usize::try_from(stride)
        .ok()
        .and_then(|stride| {
            usize::try_from(height)
                .ok()
                .and_then(|height| stride.checked_mul(height))
        })
        .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
    if mapped.pData.is_null() {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    // SAFETY: D3D11 Map returned pData for at least RowPitch * height bytes until Unmap.
    let bytes = unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>(), byte_len) };
    Ok(ReadbackFrame {
        width,
        height,
        stride,
        bgra_pixels: bytes.to_vec(),
        system_relative_time_100ns: 0,
    })
}
