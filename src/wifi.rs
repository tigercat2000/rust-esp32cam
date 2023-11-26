use anyhow::Result;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripheral,
    nvs::EspDefaultNvsPartition,
    timer::EspTaskTimerService,
    wifi::{AsyncWifi, AuthMethod, ClientConfiguration, Configuration, EspWifi},
};
use log::{info, warn};

pub async fn init_wifi<'a>(
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
        if connect(ssid, pass, sysloop.clone(), &mut esp_wifi)
            .await
            .is_ok()
        {
            break;
        }
        counter += 1;
        warn!("Failed to connect to wifi, try {}", counter);
    }

    Ok(Box::new(esp_wifi))
}

pub async fn connect(
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

    let mut wifi = AsyncWifi::wrap(esp_wifi, sysloop, EspTaskTimerService::new()?)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;

    info!("Starting wifi...");

    wifi.start().await?;

    info!("Scanning...");

    let mut ap_infos = wifi.scan().await?.into_iter();

    let ours = ap_infos.find(|a| a.ssid == ssid);

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

    wifi.connect().await?;

    info!("Waiting for DHCP lease...");

    wifi.wait_netif_up().await?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;

    info!("Wifi DHCP info: {:?}", ip_info);

    Ok(())
}
