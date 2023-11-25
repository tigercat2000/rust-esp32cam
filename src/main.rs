pub mod camera;
pub mod wifi;

use anyhow::{bail, Result};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
    http::server::{Configuration, EspHttpServer},
    io::Write,
    wifi::EspWifi,
};
use log::{info, warn};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::camera::{Camera, CameraConfig, FrameSize};
use crate::wifi::init_wifi;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
}

fn init_http(cam: Arc<Mutex<Camera>>) -> Result<EspHttpServer> {
    let mut server = EspHttpServer::new(&Configuration::default())?;

    server.fn_handler("/", esp_idf_svc::http::Method::Get, move |request| {
        let mut time = Instant::now();
        let result = cam.lock().unwrap().capture_jpeg();
        info!("Took {}ms to capture_jpeg", time.elapsed().as_millis());

        match result {
            Ok(jpeg) => {
                time = Instant::now();
                let mut response = request.into_response(
                    200,
                    None,
                    &[
                        ("Content-Type", "image/jpeg"),
                        ("Content-Length", &jpeg.len().to_string()),
                    ],
                )?;

                response.write_all(&jpeg)?;
                info!("Took {}ms to send image", time.elapsed().as_millis());
            }
            Err(e) => {
                let mut response = request.into_status_response(500)?;
                writeln!(response, "Error: {:#?}", e)?;
            }
        }

        Ok(())
    })?;

    Ok(server)
}

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;

    let mut cam_config = CameraConfig::new_jpeg_ov2640();
    cam_config.frame_size = FrameSize::UXGA;

    let camera = Camera::new(
        cam_config,
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

    let wifi = init_wifi(
        CONFIG.wifi_ssid,
        CONFIG.wifi_psk,
        peripherals.modem,
        sysloop.clone(),
    )?;

    let _server = init_http(cam)?;

    main_loop(wifi, sysloop)
}

fn main_loop(mut wifi: Box<EspWifi<'_>>, sysloop: EspSystemEventLoop) -> Result<()> {
    'main: loop {
        match wifi.is_up() {
            Ok(false) | Err(_) => {
                warn!("WiFi died, attempting to reconnect...");
                let mut counter = 0;
                loop {
                    if wifi::connect(
                        CONFIG.wifi_ssid,
                        CONFIG.wifi_psk,
                        sysloop.clone(),
                        &mut wifi,
                    )
                    .is_ok()
                    {
                        info!("WiFi reconnected successfully.");
                        break;
                    }
                    counter += 1;
                    warn!("Failed to connect to wifi, attempt {}", counter);

                    // If we fail to connect for long enough, reset the damn processor
                    if counter > 10 {
                        break 'main;
                    }
                }
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    bail!("Something went horribly wrong!!!")
}
