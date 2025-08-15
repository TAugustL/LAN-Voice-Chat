use super::Opt;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host, StreamConfig};
use std::error::Error;

/// Normalizes the audio data and filters out most noise.
pub fn normalize(vector: &[f32]) -> Vec<f32> {
    // filter out (most) noise
    let mut vector: Vec<f32> = vector
        .iter()
        .map(|f| if f.abs() < 1e-4f32 { &0.0f32 } else { f })
        .copied()
        .collect();

    if vector.iter().all(|f| *f == 0.0f32) {
        vector = Vec::new();
    }

    let mut min: f32 = 100.0;
    let mut max: f32 = -100.0;
    for i in &vector {
        let f = *i;
        if f < min {
            min = f;
        }
        if f > max {
            max = f;
        }
    }

    let norm = |f: &f32| 2.0 * ((f - min) / (max - min)) - 1.0; // [-1.0, 1.0]
    vector.iter().map(norm).collect()
}

/// Converts the byte array sent over the TcpStream to audio data (Vec<f32>).
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

/// Set up the input device and stream with the default input config.
pub fn get_input_device(audio_host: &Host, opt: &Opt) -> Result<Device, Box<dyn Error>> {
    let input_device = if opt.input_device == "default" {
        audio_host.default_input_device()
    } else {
        audio_host
            .input_devices()?
            .find(|x| x.name().map(|y| y == opt.input_device).unwrap_or(false))
    }
    .expect("Failed to find input device!");
    println!("Input device: {}", input_device.name()?);
    Ok(input_device)
}

/// Set up the output device and stream with the default output config.
pub fn get_output_device(audio_host: &Host, opt: &Opt) -> Result<Device, Box<dyn Error>> {
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

/// Get the input config for the input device.
pub fn get_input_config(device: &Device) -> StreamConfig {
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

/// Get the output config for the output device.
pub fn get_output_config(device: &Device) -> StreamConfig {
    let mut supported_configs_range = device
        .supported_output_configs()
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
