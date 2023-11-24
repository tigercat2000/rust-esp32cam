mod wifi;

use anyhow::{anyhow, Result};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        gpio::{IOPin, InputPin},
        ledc::{LedcChannel, LedcTimer},
        peripheral::Peripheral,
        peripherals::Peripherals,
    },
    http::server::{Configuration, EspHttpServer},
    io::Write,
    sys::{
        cam::{
            camera_config_t, camera_config_t__bindgen_ty_2,
            camera_fb_location_t_CAMERA_FB_IN_PSRAM, camera_fb_t,
            camera_grab_mode_t_CAMERA_GRAB_WHEN_EMPTY, esp_camera_deinit, esp_camera_fb_get,
            esp_camera_fb_return, esp_camera_init, frame2jpg, framesize_t_FRAMESIZE_UXGA,
            pixformat_t_PIXFORMAT_JPEG, pixformat_t_PIXFORMAT_RAW, pixformat_t_PIXFORMAT_RGB555,
            pixformat_t_PIXFORMAT_RGB565,
        },
        esp, EspError,
    },
};
use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::wifi::wifi;

struct Camera<'s> {
    _phantom: PhantomData<&'s ()>,
    _config: camera_config_t,
}

impl<'s> Camera<'s> {
    // TODO: specify IO requirements more accurately
    #[allow(clippy::too_many_arguments)]
    pub fn new<T: LedcTimer, C: LedcChannel>(
        pin_pwdn: impl Peripheral<P = impl IOPin> + 's,
        pin_xclk: impl Peripheral<P = impl IOPin> + 's,
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
        pin_vsync: impl Peripheral<P = impl IOPin> + 's,
        pin_href: impl Peripheral<P = impl IOPin> + 's,
        pin_pclk: impl Peripheral<P = impl IOPin> + 's,
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
            // Disable powerdown and reset
            pin_pwdn,
            pin_reset: -1,

            pin_xclk,
            __bindgen_anon_1: esp_idf_svc::sys::cam::camera_config_t__bindgen_ty_1 { pin_sccb_sda },
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

            xclk_freq_hz: 20000000,
            ledc_timer: T::timer(),
            ledc_channel: C::channel(),

            pixel_format: pixformat_t_PIXFORMAT_JPEG,
            frame_size: framesize_t_FRAMESIZE_UXGA,

            jpeg_quality: 5,
            fb_count: 2,
            fb_location: camera_fb_location_t_CAMERA_FB_IN_PSRAM,
            grab_mode: camera_grab_mode_t_CAMERA_GRAB_WHEN_EMPTY,

            sccb_i2c_port: Default::default(),
        };

        esp!(unsafe { esp_camera_init(&config) })?;

        Ok(Self {
            _phantom: PhantomData,
            _config: config,
        })
    }

    // This is mutable because we're holding a lock to the camera framebuffer
    #[allow(dead_code)]
    pub fn capture_cb<F>(&mut self, f: F) -> Result<()>
    where
        F: Fn(*mut camera_fb_t),
    {
        let fb = unsafe { esp_camera_fb_get() };
        if fb.is_null() {
            return Err(anyhow!("Failed to get camera framebuffer"));
        }

        f(fb);

        unsafe { esp_camera_fb_return(fb) };

        Ok(())
    }

    pub unsafe fn capture_jpeg(&mut self) -> Result<Vec<u8>> {
        let fb_raw = esp_camera_fb_get();
        if fb_raw.is_null() {
            return Err(anyhow!("Failed to get camera framebuffer"));
        }

        let mut buffer: *mut u8 = std::ptr::null_mut();
        let mut buffer_len: usize = 0;

        let fb = fb_raw.as_mut().unwrap();
        if fb.format != pixformat_t_PIXFORMAT_JPEG {
            let converted = frame2jpg(fb_raw, 80, &mut buffer, &mut buffer_len);
            if !converted {
                esp_camera_fb_return(fb_raw);
                return Err(anyhow!("Unable to convert to JPEG"));
            }
        } else {
            buffer = fb.buf;
            buffer_len = fb.len;
        }

        let vec_clone = std::slice::from_raw_parts_mut(buffer, buffer_len).to_vec();
        esp_camera_fb_return(fb_raw);
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

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
}

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take()?;

    // The constant `CONFIG` is auto-generated by `toml_config`.
    let app_config = CONFIG;

    // Connect to the Wi-Fi network
    let _wifi = wifi(
        app_config.wifi_ssid,
        app_config.wifi_psk,
        peripherals.modem,
        sysloop,
    )?;

    let camera = Camera::new(
        &mut peripherals.pins.gpio32,
        &mut peripherals.pins.gpio0,
        &mut peripherals.pins.gpio26,
        &mut peripherals.pins.gpio27,
        &mut peripherals.pins.gpio35,
        &mut peripherals.pins.gpio34,
        &mut peripherals.pins.gpio39,
        &mut peripherals.pins.gpio36,
        &mut peripherals.pins.gpio21,
        &mut peripherals.pins.gpio19,
        &mut peripherals.pins.gpio18,
        &mut peripherals.pins.gpio5,
        &mut peripherals.pins.gpio25,
        &mut peripherals.pins.gpio23,
        &mut peripherals.pins.gpio22,
        &mut peripherals.ledc.timer0,
        &mut peripherals.ledc.channel0,
    )?;

    let cam = Arc::new(Mutex::new(camera));

    let mut server = EspHttpServer::new(&Configuration::default())?;

    server.fn_handler("/", esp_idf_svc::http::Method::Get, |request| {
        let result = unsafe { cam.lock().unwrap().capture_jpeg() };

        match result {
            Ok(jpeg) => {
                let mut response = request.into_response(
                    200,
                    None,
                    &[
                        ("Content-Type", "image/jpeg"),
                        ("Content-Length", &jpeg.len().to_string()),
                    ],
                )?;

                response.write_all(&jpeg)?;
            }
            Err(e) => {
                let mut response = request.into_status_response(500)?;
                writeln!(response, "Error: {:#?}", e)?;
            }
        }

        Ok(())
    })?;

    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
