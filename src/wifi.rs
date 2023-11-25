use anyhow::Result;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripheral,
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::{info, warn};

pub fn init_wifi<'a>(
    ssid: &str,
    pass: &str,
    modem: impl peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'a,
    sysloop: EspSystemEventLoop,
) -> Result<Box<EspWifi<'a>>> {
    let mut esp_wifi = EspWifi::new(
        modem,
        sysloop.clone(),
        Some(EspDefaultNvsPartition::take()?),
    )?;

    let mut counter = 0;

    loop {
        if connect(ssid, pass, sysloop.clone(), &mut esp_wifi).is_ok() {
            break;
        }
        counter += 1;
        warn!("Failed to connect to wifi, {} try", counter);
    }

    Ok(Box::new(esp_wifi))
}

pub fn connect(
    ssid: &str,
    pass: &str,
    sysloop: EspSystemEventLoop,
    esp_wifi: &mut EspWifi<'_>,
) -> Result<()> {
    if ssid.is_empty() {
        panic!("Missing WiFi name")
    }

    let auth_method = if pass.is_empty() {
        info!("Wifi password is empty");
        AuthMethod::None
    } else {
        AuthMethod::WPA2Personal
    };

    let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;

    info!("Starting wifi...");

    wifi.start()?;

    info!("Scanning...");

    let ap_infos = wifi.scan()?.into_iter();

    info!("Scan found: {:#?}", ap_infos);

    let ours = ap_infos.into_iter().find(|a| a.ssid == ssid);

    let channel = if let Some(ours) = ours {
        info!(
            "Found configured access point {} on channel {}",
            ssid, ours.channel
        );
        Some(ours.channel)
    } else {
        info!(
            "Configured access point {} not found during scanning, will go with unknown channel",
            ssid
        );
        None
    };

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid.into(),
        password: pass.into(),
        channel,
        auth_method,
        ..Default::default()
    }))?;

    info!("Connecting wifi...");

    wifi.connect()?;

    info!("Waiting for DHCP lease...");

    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;

    info!("Wifi DHCP info: {:?}", ip_info);

    Ok(())
}
