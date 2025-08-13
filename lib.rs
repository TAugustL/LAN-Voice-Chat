#![forbid(unsafe_code)]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, StreamConfig};

use std::error::Error;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

struct Opt {
    /// The audio devices to use
    input_device: String,
    output_device: String,

    /// Use the JACK host
    #[allow(dead_code)]
    jack: bool,
}

impl Opt {
    fn new() -> Self {
        let args: Vec<String> = std::env::args().collect();
        Opt {
            input_device: args.get(3).unwrap_or(&String::from("default")).to_string(),
            output_device: args.get(4).unwrap_or(&String::from("default")).to_string(),
            jack: cfg!(all(
                any(
                    target_os = "linux",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "netbsd"
                ),
                feature = "jack"
            )),
        }
    }
}

const SLEEP_DURATION: std::time::Duration = std::time::Duration::from_secs(2);

pub struct Client {
    pub address: String,
    input_device: Device,
    output_device: Device,
    config: StreamConfig,
}

impl Client {
    pub fn new(address: String) -> Result<Self, Box<dyn Error>> {
        let opt = Opt::new();
        let audio_host = get_audio_host(&opt);
        let input_device = get_input_device(&audio_host, &opt)?;
        let output_device = get_output_device(&audio_host, &opt)?;
        let config = get_device_config(&input_device);

        Ok(Client {
            address,
            input_device,
            output_device,
            config,
        })
    }

    async fn chat(&mut self, mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
        println!("Entering chat...\n");
        stream.set_nonblocking(true).unwrap();

        loop {
            let mut buffer: Vec<u8> = vec![
                0;
                SLEEP_DURATION.as_secs() as usize
                    * 4
                    * (self.config.sample_rate.0 * self.config.channels as u32)
                        as usize
            ];
            if let Ok(_) = stream.read_exact(&mut buffer) {
                println!(
                    "Received bytes! ({} non-zero)",
                    buffer.iter().filter(|e| **e != 0).count()
                );
            }
            let audio_data = buffer_to_audio_data(&buffer);

            // Play audio
            let mut i: usize = 0;
            let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for sample in data {
                    *sample = *audio_data.get(i).unwrap_or(&0.0);
                    i += 1;
                }
            };
            let output_stream = self
                .output_device
                .build_output_stream(
                    &self.config,
                    output_data_fn,
                    |e| eprintln!("Stream error: {e}"),
                    None,
                )
                .unwrap();
            output_stream.play().unwrap();

            let input_samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(
                (self.config.sample_rate.0 * self.config.channels as u32) as usize,
            )));
            let input_samples_ref = input_samples.clone();

            const VOLUME: f32 = 10.0;
            // TODO: normalize data
            let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut lock) = input_samples_ref.try_lock() {
                    let buffer: &mut Vec<f32> = lock.as_mut();
                    buffer
                        .extend_from_slice(&data.iter().map(|f| f * VOLUME).collect::<Vec<f32>>());
                }
            };

            // Record samples
            let input_stream = self.input_device.build_input_stream(
                &self.config,
                input_data_fn,
                |e| eprintln!("Stream error: {e}"),
                None,
            )?;
            input_stream.play()?;
            thread::sleep(SLEEP_DURATION);

            // Send Samples
            if let Ok(inner) = input_samples.lock() {
                let mut fixed_data_buffer: Vec<u8> = Vec::with_capacity(inner.len() * 4);
                for f in &inner.to_vec() {
                    fixed_data_buffer.extend_from_slice(&f.to_le_bytes());
                }
                if let Ok(_) = stream.write_all(&fixed_data_buffer) {
                    println!("Sent bytes!");
                }
            }
        }
    }

    pub async fn listen(&mut self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.address)?;
        let stream = listener.accept()?.0;
        self.chat(stream).await?;
        Ok(())
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn Error>> {
        let stream = TcpStream::connect(&self.address)?;
        self.chat(stream).await?;
        Ok(())
    }
}

// Helper functions

fn buffer_to_audio_data(buffer: &[u8]) -> Vec<f32> {
    let mut audio_data: Vec<f32> = Vec::with_capacity(buffer.len() / 4);
    for i in (0..buffer.len()).step_by(4) {
        let flt = f32::from_le_bytes([buffer[i], buffer[i + 1], buffer[i + 2], buffer[i + 3]]);
        audio_data.push(flt);
    }
    audio_data
}

#[allow(unused_variables)]
fn get_audio_host(opt: &Opt) -> Host {
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

fn get_input_device(audio_host: &Host, opt: &Opt) -> Result<Device, Box<dyn Error>> {
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

fn get_output_device(audio_host: &Host, opt: &Opt) -> Result<Device, Box<dyn Error>> {
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

fn get_device_config(device: &Device) -> StreamConfig {
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
