use super::Opt;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host, StreamConfig};
use std::error::Error;

// Helper functions

pub fn normalize(vector: &[f32]) -> Vec<f32> {
    let sum: f32 = vector.iter().sum();
    vector.iter().map(|e| *e / sum).collect()
}

pub fn buffer_to_audio_data(buffer: &[u8]) -> Vec<f32> {
    let mut audio_data: Vec<f32> = Vec::with_capacity(buffer.len() / 4);
    for i in (0..buffer.len()).step_by(4) {
        let flt = f32::from_le_bytes([buffer[i], buffer[i + 1], buffer[i + 2], buffer[i + 3]]);
        audio_data.push(flt);
    }
    audio_data
}

#[allow(unused_variables)]
pub fn get_audio_host(opt: &Opt) -> Host {
    // Conditionally compile with jack if the feature is specified.
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    // Manually check for flags. Can be passed through cargo with -- e.g.
    // cargo run --release --example beep --features jack -- --jack
    let audio_host = if opt.jack {
        println!("HINT: using jack");
        cpal::host_from_id(cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .expect(
                "Make sure --features jack is specified. Only works on OSes where jack is available!",
            )).expect("jack host unavailable!")
    } else {
        cpal::default_host()
    };
    #[cfg(any(
        not(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        )),
        not(feature = "jack")
    ))]
    let audio_host = cpal::default_host();

    audio_host
}

pub fn get_input_device(audio_host: &Host, opt: &Opt) -> Result<Device, Box<dyn Error>> {
    // Set up the input device and stream with the default input config.
    let input_device = if opt.input_device == "default" {
        audio_host.default_input_device()
    } else {
        audio_host
            .input_devices()?
            .find(|x| x.name().map(|y| y == opt.input_device).unwrap_or(false))
    }
    .expect("Failed to find input input_device!");
    println!("Input device: {}", input_device.name()?);
    Ok(input_device)
}

pub fn get_output_device(audio_host: &Host, opt: &Opt) -> Result<Device, Box<dyn Error>> {
    // Set up the output input_device and stream with the default output config.
    let output_device = if opt.output_device == "default" {
        audio_host.default_output_device()
    } else {
        for dev in audio_host.output_devices()? {
            println!("{}", dev.name()?);
        }
        audio_host
            .output_devices()?
            .find(|x| x.name().map(|y| y == opt.output_device).unwrap_or(false))
    }
    .expect("Failed to find output device!");
    println!("Output device: {}", output_device.name()?);
    Ok(output_device)
}

pub fn get_device_config(device: &Device) -> StreamConfig {
    // get the device config
    // HINT: does the same for input and output devices!
    let mut supported_configs_range = device
        .supported_input_configs()
        .expect("Error while querying configs!");
    let supported_config = if let Some(cfg) = supported_configs_range
        .next()
        .expect("No supported config!")
        .try_with_sample_rate(cpal::SampleRate(22050))
    {
        cfg
    } else {
        eprintln!("Failed to use 22.05 kHz SR!");
        supported_configs_range
            .next()
            .expect("No supported config!")
            .with_max_sample_rate()
    };
    let config: StreamConfig = supported_config.into();
    config
}
