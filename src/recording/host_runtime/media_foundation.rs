//! Media Foundation runtime availability adapter.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaFoundationAvailability {
    Available,
    UnsupportedPlatform,
    #[cfg_attr(
        not(any(windows, test)),
        expect(
            dead_code,
            reason = "Windows Media Foundation adapter constructs runtime-unavailable status"
        )
    )]
    RuntimeUnavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VideoEncoderAvailability {
    pub(crate) h264_available: bool,
    pub(crate) h264_hardware_available: bool,
}

pub(crate) fn media_foundation_availability() -> MediaFoundationAvailability {
    platform_media_foundation_availability()
}

pub(crate) fn video_encoder_availability() -> VideoEncoderAvailability {
    platform_video_encoder_availability()
}

#[cfg(windows)]
mod platform {
    #![allow(unsafe_code)]

    use windows::Win32::{
        Media::MediaFoundation::{
            IMFActivate, MF_VERSION, MFMediaType_Video, MFSTARTUP_FULL, MFShutdown, MFStartup,
            MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_ALL, MFT_ENUM_FLAG_HARDWARE,
            MFT_ENUM_FLAG_SORTANDFILTER, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
        },
        System::Com::CoTaskMemFree,
    };

    use super::{MediaFoundationAvailability, VideoEncoderAvailability};

    #[derive(Debug)]
    pub(crate) struct MediaFoundationRuntime;

    impl MediaFoundationRuntime {
        pub(crate) fn startup() -> windows::core::Result<Self> {
            // SAFETY: MFStartup initializes Media Foundation for this process with the documented
            // full startup mode. The returned RAII guard calls MFShutdown exactly once on drop.
            unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }?;
            Ok(Self)
        }
    }

    impl Drop for MediaFoundationRuntime {
        fn drop(&mut self) {
            // SAFETY: this drops the runtime guard created by a successful MFStartup call.
            let _ = unsafe { MFShutdown() };
        }
    }

    pub(super) fn platform_media_foundation_availability() -> MediaFoundationAvailability {
        match MediaFoundationRuntime::startup() {
            Ok(_runtime) => MediaFoundationAvailability::Available,
            Err(_) => MediaFoundationAvailability::RuntimeUnavailable,
        }
    }

    pub(super) fn platform_video_encoder_availability() -> VideoEncoderAvailability {
        let Ok(_runtime) = MediaFoundationRuntime::startup() else {
            return VideoEncoderAvailability::default();
        };
        VideoEncoderAvailability {
            h264_available: h264_encoder_available(MFT_ENUM_FLAG_ALL | MFT_ENUM_FLAG_SORTANDFILTER),
            h264_hardware_available: h264_encoder_available(
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            ),
        }
    }

    fn h264_encoder_available(
        flags: windows::Win32::Media::MediaFoundation::MFT_ENUM_FLAG,
    ) -> bool {
        let output_type = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H264,
        };
        let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0_u32;
        // SAFETY: Media Foundation is initialized by the caller. `output_type` lives for this
        // call, output pointers are valid, and the returned activation array is released below with
        // CoTaskMemFree after dropping each COM interface.
        let result = unsafe {
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                flags,
                None,
                Some(&output_type),
                &mut activates,
                &mut count,
            )
        };
        if result.is_err() || activates.is_null() {
            return false;
        }
        let available = count > 0;
        release_mft_activations(activates, count);
        available
    }

    fn release_mft_activations(activates: *mut Option<IMFActivate>, count: u32) {
        let Ok(len) = usize::try_from(count) else {
            // SAFETY: this is the allocation returned by MFTEnumEx. If the count is invalid for
            // this process, free the array allocation without walking it.
            unsafe { CoTaskMemFree(Some(activates.cast())) };
            return;
        };
        // SAFETY: MFTEnumEx returned `count` contiguous activation entries at this pointer.
        let items = unsafe { std::slice::from_raw_parts_mut(activates, len) };
        for item in items {
            let _ = item.take();
        }
        // SAFETY: this frees the activation array allocated by MFTEnumEx after interfaces dropped.
        unsafe { CoTaskMemFree(Some(activates.cast())) };
    }
}

#[cfg(windows)]
pub(crate) use platform::MediaFoundationRuntime;
#[cfg(windows)]
use platform::{platform_media_foundation_availability, platform_video_encoder_availability};

#[cfg(not(windows))]
const fn platform_media_foundation_availability() -> MediaFoundationAvailability {
    MediaFoundationAvailability::UnsupportedPlatform
}

#[cfg(not(windows))]
const fn platform_video_encoder_availability() -> VideoEncoderAvailability {
    VideoEncoderAvailability {
        h264_available: false,
        h264_hardware_available: false,
    }
}
