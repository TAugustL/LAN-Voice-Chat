#![forbid(unsafe_code)]

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, StreamConfig};

use std::error::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

mod util;
use util::{
    buffer_to_audio_data, get_audio_host, get_input_config, get_input_device, get_output_config,
    get_output_device, normalize,
};

pub struct Opt {
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

const VOLUME: f32 = 1.0;
const SLEEP_DURATION: std::time::Duration = std::time::Duration::from_secs(1);

pub struct Client {
    pub address: String,
    input_device: Device,
    input_config: StreamConfig,
    output_device: Device,
    output_config: StreamConfig,
}

impl Client {
    pub fn new(address: String) -> Result<Self, Box<dyn Error>> {
        let opt = Opt::new();
        let audio_host = get_audio_host(&opt);
        let input_device = get_input_device(&audio_host, &opt)?;
        let input_config = get_input_config(&input_device);
        let output_device = get_output_device(&audio_host, &opt)?;
        let output_config = get_output_config(&output_device);

        Ok(Client {
            address,
            input_device,
            input_config,
            output_device,
            output_config,
        })
    }

    async fn chat(&mut self, mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
        println!("Entering chat...\n");
        stream.set_nonblocking(true)?;

        let buffer_size = (4
            * SLEEP_DURATION.as_secs() as u32
            * self.output_config.sample_rate.0
            * self.output_config.channels as u32) as usize;

        loop {
            let mut buffer: Vec<u8> = vec![0; buffer_size];
            if stream.read_exact(&mut buffer).is_ok() {
                // println!("Received bytes!");
            }

            let audio_data = buffer_to_audio_data(&buffer);

            // Collect output audio
            let mut i: usize = 0;
            let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for sample in data {
                    *sample = *audio_data.get(i).unwrap_or(&0.0);
                    i += 1;
                }
            };
            let output_stream = self.output_device.build_output_stream(
                &self.output_config,
                output_data_fn,
                |e| eprintln!("Stream error: {e}"),
                None,
            )?;

            // Record input audio
            let input_samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(
                (SLEEP_DURATION.as_secs() as u32
                    * self.input_config.sample_rate.0
                    * self.input_config.channels as u32) as usize,
            )));
            let input_samples_ref = input_samples.clone();

            let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut lock) = input_samples_ref.try_lock() {
                    let buffer: &mut Vec<f32> = lock.as_mut();
                    let norm_data = normalize(data);
                    let final_data: Vec<f32> = norm_data.iter().map(|f| f * VOLUME).collect();
                    buffer.extend_from_slice(&final_data);
                }
            };
            let input_stream = self.input_device.build_input_stream(
                &self.input_config,
                input_data_fn,
                |e| eprintln!("Stream error: {e}"),
                None,
            )?;

            output_stream.play()?; // start playing
            input_stream.play()?; // start recording

            thread::sleep(SLEEP_DURATION);

            // Send Samples
            if let Ok(inner) = input_samples.lock() {
                let mut fixed_data_buffer: Vec<u8> = Vec::with_capacity(inner.len() * 4);
                for f in &inner.to_vec() {
                    fixed_data_buffer.extend_from_slice(&f.to_le_bytes());
                }
                if stream.write_all(&fixed_data_buffer).is_ok() {
                    // println!("Sent bytes!");
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
        if stream.peer_addr()?.ip() == stream.local_addr()?.ip() {
            eprintln!(
                "\nWARNING: It seems like you are connecting to yourself. Unless you specefied different output devices for the the chat instances, you may hear a lot of noise and echoes.\n"
            );
        }
        self.chat(stream).await?;
        Ok(())
    }
}
