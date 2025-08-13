#![forbid(unsafe_code)]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, StreamConfig};

use std::error::Error;
use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
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

    // Get the voice data from the other person
    async fn spawn_io_thread(
        mut stream: TcpStream,
        msg_channel: (Sender<Vec<f32>>, Receiver<Vec<f32>>),
        output_device: Device,
        config: StreamConfig,
    ) -> Sender<Vec<f32>> {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let (input_sender, input_receiver) = channel::<Vec<f32>>();

        thread::spawn(move || {
            loop {
                // Receive audio data
                if let Ok(mut voice_data) = msg_channel.1.try_recv() {
                    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        for sample in data {
                            *sample = voice_data.pop().unwrap_or(0.0);
                        }
                    };

                    // Play audio
                    let output_stream = output_device
                        .build_output_stream(&config, output_data_fn, err_fn, None)
                        .unwrap();
                    output_stream.play().unwrap();
                    thread::sleep(SLEEP_DURATION);
                }

                let mut buffer: Vec<u8> = Vec::new();
                if let Ok(n) = reader.read(&mut buffer) {
                    println!("received buffer len: {n}");
                }

                match input_receiver.try_recv() {
                    Ok(data) => {
                        let mut fixed_data_buffer: Vec<u8> = Vec::with_capacity(data.len() * 4);
                        for f in &data {
                            fixed_data_buffer.extend_from_slice(&f.to_le_bytes());
                        }
                        println!("fixed data: {}", fixed_data_buffer.len());
                        msg_channel.0.send(data).unwrap();
                        // SEND DATA
                        match stream.write_all(&fixed_data_buffer) {
                            Ok(_) => (),
                            Err(_err) => return,
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        panic!("Channel disconnected!")
                    }
                }
                thread::sleep(SLEEP_DURATION);
            }
        });
        input_sender
    }

    async fn chat(&mut self, stream: TcpStream) -> Result<(), Box<dyn Error>> {
        println!("Entering chat...\n");
        stream.set_nonblocking(true).unwrap();
        let send_channel = channel::<Vec<f32>>();
        let recv_chennel = channel::<Vec<f32>>();

        let input_sender = Self::spawn_io_thread(
            stream,
            (send_channel.0, recv_chennel.1),
            self.output_device.clone(),
            self.config.clone(),
        )
        .await;

        loop {
            // Play audio
            let audio_data = send_channel.1.try_recv().unwrap_or(Vec::new());
            let mut i: usize = 0;
            let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for sample in data {
                    *sample = *audio_data.get(i).unwrap_or(&0.0);
                    i += 1;
                }
            };
            let output_stream = self
                .output_device
                .build_output_stream(&self.config, output_data_fn, err_fn, None)
                .unwrap();
            output_stream.play().unwrap();

            let input_samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(
                (self.config.sample_rate.0 * self.config.channels as u32) as usize,
            )));
            let input_samples_ref = input_samples.clone();

            let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut lock = input_samples_ref.try_lock().unwrap();
                let buffer: &mut Vec<f32> = lock.as_mut();
                buffer.extend_from_slice(data);
            };

            // Record samples
            let input_stream =
                self.input_device
                    .build_input_stream(&self.config, input_data_fn, err_fn, None)?;
            input_stream.play()?;
            thread::sleep(SLEEP_DURATION);

            // Send Samples
            if let Ok(inner) = input_samples.lock() {
                println!("Input samples sent: {}", inner.len());
                if !inner.is_empty() {
                    match input_sender.send(inner.to_vec()) {
                        Ok(_) => (),
                        Err(err) => panic!("Failed to send sample: {err}"),
                    }
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

fn err_fn(err: cpal::StreamError) {
    eprintln!("An error occurred on stream: {err}");
}
