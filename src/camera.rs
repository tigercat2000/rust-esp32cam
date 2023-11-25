use anyhow::{anyhow, Result};
use esp_idf_svc::{
    hal::{
        gpio::{IOPin, InputPin, OutputPin},
        ledc::{LedcChannel, LedcTimer},
        peripheral::Peripheral,
        units::Hertz,
    },
    sys::{
        cam::{
            self, camera_config_t, camera_config_t__bindgen_ty_1, camera_config_t__bindgen_ty_2,
            esp_camera_deinit, esp_camera_fb_get, esp_camera_fb_return, esp_camera_init, frame2bmp,
            frame2jpg,
        },
        esp, EspError,
    },
};
use std::{marker::PhantomData, ptr::NonNull};

#[derive(Clone, Copy, Debug)]
pub struct CameraConfig {
    pub xclk_freq: Hertz,
    pub pixel_format: PixelFormat,
    pub frame_size: FrameSize,
    pub jpeg_quality: i32,
    pub fb_count: usize,
    pub fb_location: FbLocation,
    pub grab_mode: FbGrabMode,
    pub sccb_i2c_port: Option<i32>,
}

impl CameraConfig {
    pub fn new_jpeg_ov2640() -> Self {
        Self {
            xclk_freq: Hertz::from(20000000),
            pixel_format: PixelFormat::JPEG,
            frame_size: FrameSize::UXGA,
            jpeg_quality: 12,
            fb_count: 1,
            fb_location: FbLocation::PSRAM,
            grab_mode: FbGrabMode::WhenEmpty,
            sccb_i2c_port: None,
        }
    }
}

pub struct Camera<'s> {
    _phantom: PhantomData<&'s ()>,
    _config: camera_config_t,
}

impl<'s> Camera<'s> {
    #[allow(clippy::too_many_arguments)]
    pub fn new<T: LedcTimer, C: LedcChannel>(
        camera_config: CameraConfig,
        pin_pwdn: impl Peripheral<P = impl OutputPin> + 's,
        pin_xclk: impl Peripheral<P = impl OutputPin> + 's,
        pin_sccb_sda: impl Peripheral<P = impl IOPin> + 's,
        pin_sccb_scl: impl Peripheral<P = impl IOPin> + 's,
        pin_d7: impl Peripheral<P = impl InputPin> + 's,
        pin_d6: impl Peripheral<P = impl InputPin> + 's,
        pin_d5: impl Peripheral<P = impl InputPin> + 's,
        pin_d4: impl Peripheral<P = impl InputPin> + 's,
        pin_d3: impl Peripheral<P = impl InputPin> + 's,
        pin_d2: impl Peripheral<P = impl InputPin> + 's,
        pin_d1: impl Peripheral<P = impl InputPin> + 's,
        pin_d0: impl Peripheral<P = impl InputPin> + 's,
        pin_vsync: impl Peripheral<P = impl InputPin> + 's,
        pin_href: impl Peripheral<P = impl InputPin> + 's,
        pin_pclk: impl Peripheral<P = impl InputPin> + 's,
        _ledc_timer: impl Peripheral<P = T> + 's,
        _ledc_channel: impl Peripheral<P = C> + 's,
    ) -> std::result::Result<Self, EspError> {
        let pin_pwdn = pin_pwdn.into_ref().pin();
        let pin_xclk = pin_xclk.into_ref().pin();
        let pin_sccb_sda = pin_sccb_sda.into_ref().pin();
        let pin_sccb_scl = pin_sccb_scl.into_ref().pin();
        let pin_d7 = pin_d7.into_ref().pin();
        let pin_d6 = pin_d6.into_ref().pin();
        let pin_d5 = pin_d5.into_ref().pin();
        let pin_d4 = pin_d4.into_ref().pin();
        let pin_d3 = pin_d3.into_ref().pin();
        let pin_d2 = pin_d2.into_ref().pin();
        let pin_d1 = pin_d1.into_ref().pin();
        let pin_d0 = pin_d0.into_ref().pin();
        let pin_vsync = pin_vsync.into_ref().pin();
        let pin_href = pin_href.into_ref().pin();
        let pin_pclk = pin_pclk.into_ref().pin();

        let config = camera_config_t {
            pin_pwdn,
            // Disable reset
            pin_reset: -1,

            pin_xclk,
            __bindgen_anon_1: camera_config_t__bindgen_ty_1 { pin_sccb_sda },
            __bindgen_anon_2: camera_config_t__bindgen_ty_2 { pin_sccb_scl },

            pin_d7,
            pin_d6,
            pin_d5,
            pin_d4,
            pin_d3,
            pin_d2,
            pin_d1,
            pin_d0,
            pin_vsync,
            pin_href,
            pin_pclk,

            xclk_freq_hz: camera_config.xclk_freq.0.try_into().unwrap(),
            ledc_timer: T::timer(),
            ledc_channel: C::channel(),

            pixel_format: camera_config.pixel_format.into(),
            frame_size: camera_config.frame_size.into(),

            jpeg_quality: camera_config.jpeg_quality,
            fb_count: camera_config.fb_count,
            fb_location: camera_config.fb_location.into(),
            grab_mode: camera_config.grab_mode.into(),

            // -1 means disabled
            sccb_i2c_port: camera_config.sccb_i2c_port.unwrap_or(-1),
        };

        esp!(unsafe { esp_camera_init(&config) })?;

        Ok(Self {
            _phantom: PhantomData,
            _config: config,
        })
    }

    pub fn capture_jpeg(&mut self) -> Result<Vec<u8>> {
        // Safety: This is already an exclusive reference inside the camera library
        let mut fb_raw = NonNull::new(unsafe { esp_camera_fb_get() })
            .ok_or(anyhow!("Failed to get camera framebuffer"))?;

        let fb = unsafe { fb_raw.as_mut() };

        let mut buffer: *mut u8 = std::ptr::null_mut();
        let mut buffer_len: usize = 0;

        if fb.format != cam::pixformat_t_PIXFORMAT_JPEG {
            let converted = unsafe { frame2jpg(fb, 80, &mut buffer, &mut buffer_len) };
            if !converted {
                unsafe { esp_camera_fb_return(fb) };
                return Err(anyhow!("Unable to convert to JPEG"));
            }
        } else {
            buffer = fb.buf;
            buffer_len = fb.len;
        }

        let vec_clone = unsafe { std::slice::from_raw_parts_mut(buffer, buffer_len) }.to_vec();
        unsafe { esp_camera_fb_return(fb) };
        Ok(vec_clone)
    }

    pub fn capture_bmp(&mut self) -> Result<Vec<u8>> {
        // Safety: This is already an exclusive reference inside the camera library
        let mut fb_raw = NonNull::new(unsafe { esp_camera_fb_get() })
            .ok_or(anyhow!("Failed to get camera framebuffer"))?;

        let fb = unsafe { fb_raw.as_mut() };

        let mut buffer: *mut u8 = std::ptr::null_mut();
        let mut buffer_len: usize = 0;

        let converted = unsafe { frame2bmp(fb, &mut buffer, &mut buffer_len) };
        if !converted {
            unsafe { esp_camera_fb_return(fb) };
            return Err(anyhow!("Unable to convert to BMP"));
        }

        let vec_clone = unsafe { std::slice::from_raw_parts_mut(buffer, buffer_len) }.to_vec();
        unsafe { esp_camera_fb_return(fb) };
        Ok(vec_clone)
    }
}

impl<'s> Drop for Camera<'s> {
    fn drop(&mut self) {
        unsafe {
            esp_camera_deinit();
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(clippy::upper_case_acronyms)]
pub enum PixelFormat {
    RGB565,
    YUV422,
    YUV420,
    GRAYSCALE,
    JPEG,
    RGB888,
    RAW,
    RGB444,
    RGB555,
}

impl From<PixelFormat> for u32 {
    fn from(value: PixelFormat) -> Self {
        match value {
            PixelFormat::RGB565 => cam::pixformat_t_PIXFORMAT_RGB565,
            PixelFormat::YUV422 => cam::pixformat_t_PIXFORMAT_YUV422,
            PixelFormat::YUV420 => cam::pixformat_t_PIXFORMAT_YUV420,
            PixelFormat::GRAYSCALE => cam::pixformat_t_PIXFORMAT_GRAYSCALE,
            PixelFormat::JPEG => cam::pixformat_t_PIXFORMAT_JPEG,
            PixelFormat::RGB888 => cam::pixformat_t_PIXFORMAT_RGB888,
            PixelFormat::RAW => cam::pixformat_t_PIXFORMAT_RAW,
            PixelFormat::RGB444 => cam::pixformat_t_PIXFORMAT_RGB444,
            PixelFormat::RGB555 => cam::pixformat_t_PIXFORMAT_RGB555,
        }
    }
}

impl TryFrom<u32> for PixelFormat {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            cam::pixformat_t_PIXFORMAT_RGB565 => Ok(Self::RGB565),
            cam::pixformat_t_PIXFORMAT_YUV422 => Ok(Self::YUV422),
            cam::pixformat_t_PIXFORMAT_YUV420 => Ok(Self::YUV420),
            cam::pixformat_t_PIXFORMAT_GRAYSCALE => Ok(Self::GRAYSCALE),
            cam::pixformat_t_PIXFORMAT_JPEG => Ok(Self::JPEG),
            cam::pixformat_t_PIXFORMAT_RGB888 => Ok(Self::RGB888),
            cam::pixformat_t_PIXFORMAT_RAW => Ok(Self::RAW),
            cam::pixformat_t_PIXFORMAT_RGB444 => Ok(Self::RGB444),
            cam::pixformat_t_PIXFORMAT_RGB555 => Ok(Self::RGB555),
            _ => Err(anyhow!("Unable to convert {:#?} to pixel format", value)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(clippy::upper_case_acronyms, non_camel_case_types)]
pub enum FrameSize {
    S96X96,
    QQVGA,
    QCIF,
    HQVGA,
    S240X240,
    QVGA,
    CIF,
    HVGA,
    VGA,
    SVGA,
    XGA,
    HD,
    SXGA,
    UXGA,
    FHD,
    P_HD,
    P_3MP,
    QXGA,
    QHD,
    WQXGA,
    P_FHD,
    QSXGA,
    INVALID,
}

impl From<FrameSize> for u32 {
    fn from(value: FrameSize) -> u32 {
        match value {
            FrameSize::S96X96 => cam::framesize_t_FRAMESIZE_96X96,
            FrameSize::QQVGA => cam::framesize_t_FRAMESIZE_QQVGA,
            FrameSize::QCIF => cam::framesize_t_FRAMESIZE_QCIF,
            FrameSize::HQVGA => cam::framesize_t_FRAMESIZE_HQVGA,
            FrameSize::S240X240 => cam::framesize_t_FRAMESIZE_240X240,
            FrameSize::QVGA => cam::framesize_t_FRAMESIZE_QVGA,
            FrameSize::CIF => cam::framesize_t_FRAMESIZE_CIF,
            FrameSize::HVGA => cam::framesize_t_FRAMESIZE_HVGA,
            FrameSize::VGA => cam::framesize_t_FRAMESIZE_VGA,
            FrameSize::SVGA => cam::framesize_t_FRAMESIZE_SVGA,
            FrameSize::XGA => cam::framesize_t_FRAMESIZE_XGA,
            FrameSize::HD => cam::framesize_t_FRAMESIZE_HD,
            FrameSize::SXGA => cam::framesize_t_FRAMESIZE_SXGA,
            FrameSize::UXGA => cam::framesize_t_FRAMESIZE_UXGA,
            FrameSize::FHD => cam::framesize_t_FRAMESIZE_FHD,
            FrameSize::P_HD => cam::framesize_t_FRAMESIZE_P_HD,
            FrameSize::P_3MP => cam::framesize_t_FRAMESIZE_P_3MP,
            FrameSize::QXGA => cam::framesize_t_FRAMESIZE_QXGA,
            FrameSize::QHD => cam::framesize_t_FRAMESIZE_QHD,
            FrameSize::WQXGA => cam::framesize_t_FRAMESIZE_WQXGA,
            FrameSize::P_FHD => cam::framesize_t_FRAMESIZE_P_FHD,
            FrameSize::QSXGA => cam::framesize_t_FRAMESIZE_QSXGA,
            FrameSize::INVALID => cam::framesize_t_FRAMESIZE_INVALID,
        }
    }
}

impl TryFrom<u32> for FrameSize {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            cam::framesize_t_FRAMESIZE_96X96 => Ok(Self::S96X96),
            cam::framesize_t_FRAMESIZE_QQVGA => Ok(Self::QQVGA),
            cam::framesize_t_FRAMESIZE_QCIF => Ok(Self::QCIF),
            cam::framesize_t_FRAMESIZE_HQVGA => Ok(Self::HQVGA),
            cam::framesize_t_FRAMESIZE_240X240 => Ok(Self::S240X240),
            cam::framesize_t_FRAMESIZE_QVGA => Ok(Self::QVGA),
            cam::framesize_t_FRAMESIZE_CIF => Ok(Self::CIF),
            cam::framesize_t_FRAMESIZE_HVGA => Ok(Self::HVGA),
            cam::framesize_t_FRAMESIZE_VGA => Ok(Self::VGA),
            cam::framesize_t_FRAMESIZE_SVGA => Ok(Self::SVGA),
            cam::framesize_t_FRAMESIZE_XGA => Ok(Self::XGA),
            cam::framesize_t_FRAMESIZE_HD => Ok(Self::HD),
            cam::framesize_t_FRAMESIZE_SXGA => Ok(Self::SXGA),
            cam::framesize_t_FRAMESIZE_UXGA => Ok(Self::UXGA),
            cam::framesize_t_FRAMESIZE_FHD => Ok(Self::FHD),
            cam::framesize_t_FRAMESIZE_P_HD => Ok(Self::P_HD),
            cam::framesize_t_FRAMESIZE_P_3MP => Ok(Self::P_3MP),
            cam::framesize_t_FRAMESIZE_QXGA => Ok(Self::QXGA),
            cam::framesize_t_FRAMESIZE_QHD => Ok(Self::QHD),
            cam::framesize_t_FRAMESIZE_WQXGA => Ok(Self::WQXGA),
            cam::framesize_t_FRAMESIZE_P_FHD => Ok(Self::P_FHD),
            cam::framesize_t_FRAMESIZE_QSXGA => Ok(Self::QSXGA),
            cam::framesize_t_FRAMESIZE_INVALID => Ok(Self::INVALID),
            _ => Err(anyhow!("Unable to convert {:#?} to frame size", value)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(clippy::upper_case_acronyms)]
pub enum FbLocation {
    DRAM,
    PSRAM,
}

impl From<FbLocation> for u32 {
    fn from(value: FbLocation) -> Self {
        match value {
            FbLocation::DRAM => cam::camera_fb_location_t_CAMERA_FB_IN_DRAM,
            FbLocation::PSRAM => cam::camera_fb_location_t_CAMERA_FB_IN_PSRAM,
        }
    }
}

impl TryFrom<u32> for FbLocation {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            cam::camera_fb_location_t_CAMERA_FB_IN_DRAM => Ok(Self::DRAM),
            cam::camera_fb_location_t_CAMERA_FB_IN_PSRAM => Ok(Self::PSRAM),
            _ => Err(anyhow!("Unable to convert {:#?} to pixel format", value)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(clippy::upper_case_acronyms)]
pub enum FbGrabMode {
    WhenEmpty,
    Latest,
}

impl From<FbGrabMode> for u32 {
    fn from(value: FbGrabMode) -> Self {
        match value {
            FbGrabMode::WhenEmpty => cam::camera_grab_mode_t_CAMERA_GRAB_WHEN_EMPTY,
            FbGrabMode::Latest => cam::camera_grab_mode_t_CAMERA_GRAB_LATEST,
        }
    }
}

impl TryFrom<u32> for FbGrabMode {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            cam::camera_grab_mode_t_CAMERA_GRAB_WHEN_EMPTY => Ok(FbGrabMode::WhenEmpty),
            cam::camera_grab_mode_t_CAMERA_GRAB_LATEST => Ok(FbGrabMode::Latest),
            _ => Err(anyhow!("Unable to convert {:#?} to pixel format", value)),
        }
    }
}
