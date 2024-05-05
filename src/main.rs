use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;
use std::io::Read;
use std::ops::Deref;
use std::str::FromStr;
use std::{fs::File, net::SocketAddr};

use artnet_protocol::*;
use std::net::{ToSocketAddrs, UdpSocket};

use std::cmp;
use std::thread::{self, JoinHandle};
use std::sync::mpsc;
use std::time::Instant;
use std::time::Duration;

use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Debug)]
struct AddressConfig {
    address: String,
    port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct UniverseMappingConfig {
    input: (u16, u16),
    output_start: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct DeviceMappingConfig {
    host: AddressConfig,
    universes: UniverseMappingConfig,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    listen: AddressConfig,
    mappings: Vec<DeviceMappingConfig>,
}

fn read_config_file(file_path: &str) -> std::result::Result<Config, std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let config: Config = serde_json::from_str(&contents)?;
    println!("{:?}", config);
    Ok(config)
}

struct OutputDevice {
    address: String,

    // virtual screen where proxy writes the universes for passing them as a single frame to ESP
    // or depending on protocol may as well send them as multiple universes with fixed packet
    // headers and sync messages etc.
    frame: Vec<u8>,

    // Universes, which has arrived during this frame
    current_universes: HashSet<u16>,

    // Helps figuring out if some packet has gone missing. Typically with gigabit network 100kB of data should arrive in less than 1ms.
    average_micros_to_get_all_universes: f32,

    // Current sequence
    sequence: u8,

    // Number of universes configured to send to this device
    universe_count: u16,

    // Universe offset to fix when writing then to virtual screen 
    universe_offset: u16,

    // Packets to send to client... this should be accessed only by sender thread
    send_queue: Vec<Output>,

    // thread communication and the join_handle of spawned thread, filled after thread is started
    thread_tx: Option<mpsc::Sender<Output>>,
    join_handle: Option<JoinHandle<()>>

    // TODO: stats about how often actually full universe range was received
}

impl OutputDevice {
    fn new(config: &DeviceMappingConfig) -> OutputDevice {
        let universe_count = config.universes.input.1 - config.universes.input.0 + 1;
        let frame = vec![0;(universe_count as usize) * 510];

        OutputDevice {
            address: format!("{}:{}", &config.host.address, &config.host.port),
            frame,
            current_universes: HashSet::new(),
            average_micros_to_get_all_universes: 0.,
            sequence: 0,
            universe_count,
            universe_offset: config.universes.input.0,
            send_queue: Vec::new(),
            thread_tx: Option::None,
            join_handle: Option::None
        }
    }

    fn next_sequence(&mut self) -> u8 {
        if self.sequence == 255 {
            self.sequence = 1;
        } else {
            self.sequence += 1;
        }
        return self.sequence;
    }

    // TODO: maybe we need a little bit better data about universes that vec<u8>...
    fn add_universe(&mut self, mut packet: Output) {
        // TODO: get mutex access to frame

        // update destination universe
        let mut data = packet.data.as_mut().to_vec();

        // TODO: fix RGB, GRB order
        if data.len() > 510 {
            data.truncate(510);
        }

        let port: u16 = packet.port_address.into();
        let start = (port-self.universe_offset) as usize * 510;
        let end = start + data.len();
        let range = start..end;
        // println!("universe: {} to range: {:?}", port, range);
        self.frame.splice(range, data);

        // TODO: if the same universe is already in add duplicate universe error

        self.current_universes.insert(port);

        // println!("received universes {} expected amount {}", self.current_universes.len(), self.universe_count);

        // if all universes has arrived, send them forward
        if self.current_universes.len() == self.universe_count as usize {
            self.current_universes.clear();
            self.send_frame();
        }

        // TODO: if from first received universe has taken over 10ms send and add error to stats

    }

    fn send_frame(&mut self) {

        // TODO: take mutex to lock thread accessing self.frame and self.send_queue
        for universe in 0..self.universe_count {
            let start:usize = universe as usize * 510;
            let end = start + 510;
            let data: Vec<u8> = self.frame[start..end].to_vec();
            
            let mut output = Output {
                data: data.into(),
                ..Output::default()
            };
            
            // TODO: add output offset
            output.port_address = PortAddress::try_from(universe).unwrap();
            output.sequence = self.next_sequence();

            self.thread_tx.as_mut().unwrap().send(output).unwrap();
        }
    }

    fn start_output_thread(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.thread_tx = Some(tx);
        let address = self.address.to_owned();

        let join_handle = thread::spawn(move || {
            let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
            loop {
                for output in &rx {
                    // TODO: if output is Option::None break loop
                    let bytes = ArtCommand::Output(output).write_to_buffer().unwrap();
                    socket.send_to(&bytes, &address).unwrap();
                    // TODO: need to add better stats and testing how good this value is...
                    //       now most important point is to check if ESP32 is still freezing randomly
                    //       also check if there are dropped packets

                    // TODO: add stats about out going packets
                    thread::sleep(Duration::from_micros(300));
                }
                // TODO: add sync message?
            }
        });

        self.join_handle = Some(join_handle);
    }

    fn stop(&mut self) -> std::result::Result<(), String> {
        if let Some(handle) = self.join_handle.take() {
            // TODO: send stop message to exit thread loop... make value to be Option<Output>
            handle.join().map_err(|_| "Failed to join thread".to_string())
        } else {
            Ok(()) // Or handle this case differently if needed
        }
    }

}

struct Outputs {
    devices: Vec<OutputDevice>,
    device_idx_by_universe: HashMap<u16, usize>
}

impl Outputs {
    fn new(config: &Vec<DeviceMappingConfig>) -> Outputs {
        let mut devices: Vec<OutputDevice> = Vec::new();

        let mut device_by_port = HashMap::new();

        for device_config in config {
            // add mapping to setup ports which universes should be delivered to this device
            let input_range = device_config.universes.input.0..=device_config.universes.input.1;
            for port in input_range {
                // TODO: learn how to deal with multiple references to a same data and how to
                //       bind lifespan properly 
                device_by_port.insert(port, devices.len());
            }

            let mut device = OutputDevice::new(&device_config);
            device.start_output_thread();
            devices.push(device);
        }
        Outputs { devices, device_idx_by_universe: device_by_port }
    }

    fn add_universe(&mut self, packet: Output) {
        let port: u16 = packet.port_address.into();
        let device_idx = self.device_idx_by_universe.get(&port).unwrap_or(&usize::MAX);
        
        if *device_idx != usize::MAX {
            self.devices[*device_idx].add_universe(packet);
        } else {
            println!("Got unmapped universe {}", port);
        }
    }

    fn close(&mut self) {
        for device in &mut self.devices {
            device.stop().unwrap();
        }
    }
}

struct Stats {
    total_packets: u64,
    total_bytes: usize,
    packets_since_last_report: u64,
    bytes_since_last_report: usize,
    time_since_last_call: Instant,
    bucket_10: u64,
    bucket_100: u64,
    bucket_1000: u64,
    bucket_5000: u64,
    bucket_10000: u64,
    bucket_15000: u64,
    bucket_rest: u64,
    last_report_time: Instant,
}

impl Stats {
    fn new() -> Stats {
        Stats {
            total_packets: 0,
            total_bytes: 0,
            packets_since_last_report: 0,
            bytes_since_last_report: 0,
            time_since_last_call: Instant::now(),
            bucket_10: 0,
            bucket_100: 0,
            bucket_1000: 0,
            bucket_5000: 0,
            bucket_10000: 0,
            bucket_15000: 0,
            bucket_rest: 0,
            last_report_time: Instant::now(),
        }
    }

    fn log_packet(&mut self, size: &usize) {
        self.total_packets += 1;
        self.total_bytes += size;
        self.packets_since_last_report += 1;
        self.bytes_since_last_report += size;

        let elapsed_usec = self.time_since_last_call.elapsed().as_micros();

        match elapsed_usec {
            0..=10 => self.bucket_10 += 1,
            11..=100 => self.bucket_100 += 1,
            101..=1000 => self.bucket_1000 += 1,
            1001..=5000 => self.bucket_5000 += 1,
            5001..=10000 => self.bucket_10000 += 1,
            10001..=15000 => self.bucket_15000 += 1,
            _ => self.bucket_rest += 1,
        }

        self.time_since_last_call = Instant::now();

        // report every 5 secs as a side effect :likeaboss:
        if self.last_report_time.elapsed().as_secs() > 5 {
            self.report();
        }
    }

    fn report(&mut self) {
        let elapsed = self.last_report_time.elapsed();
        println!(
        "{} universes/s {:.2} Mbps Packet timings\n      0..10 usec: {} packets\n    11..100 usec: {} packets\n   101-1000 usec: {} packets\n  1001-5000 usec: {} packets\n 5001-10000 usec: {} packets\n10001-15000 usec: {} packets\n           rest : {} packets",
        (&self.packets_since_last_report * 1000000) as u128 / elapsed.as_micros(),
        (&self.bytes_since_last_report * 1000000) as f64
            / (elapsed.as_micros() as f64)
            / 1024.
            / 1024.
            * 8.,
            self.bucket_10,
            self.bucket_100,
            self.bucket_1000,
            self.bucket_5000,
            self.bucket_10000,
            self.bucket_15000,
            self.bucket_rest
            );

        self.packets_since_last_report = 0;
        self.bytes_since_last_report = 0;
        self.last_report_time = Instant::now();

        self.bucket_10 = 0;
        self.bucket_100 = 0;
        self.bucket_1000 = 0;
        self.bucket_5000 = 0;
        self.bucket_10000 = 0;
        self.bucket_15000 = 0;
        self.bucket_rest = 0;
    }
}

fn main() {
    let config = read_config_file("config.json").unwrap();
    let mut outputs = Outputs::new(&config.mappings);
    let mut stats = Stats::new();

    {
        let socket =
            UdpSocket::bind(format!("{}:{}", config.listen.address, config.listen.port)).unwrap();

        let broadcast_addr = ("255.255.255.255", 6454)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();

        socket.set_broadcast(true).unwrap();
        let buff = ArtCommand::Poll(Poll::default()).write_to_buffer().unwrap();
        socket.send_to(&buff, &broadcast_addr).unwrap();

        loop {
            let mut buffer = [0u8; 1024];
            let (length, addr) = socket.recv_from(&mut buffer).unwrap();
            let command = ArtCommand::from_buffer(&buffer[..length]).unwrap();

            stats.log_packet(&length);

            match command {
                ArtCommand::Output(output) => {
                    outputs.add_universe(output);
                }

                // TODO: invent reasonable values to poll reply
                ArtCommand::Poll(poll) => {
                    // This will most likely be our own poll request, as this is broadcast to all devices on the network
                    let command = ArtCommand::PollReply(Box::new(PollReply {
                        address: config.listen.address.parse().unwrap(),
                        port: config.listen.port,
                        version: [0, 14],
                        port_address: [255, 255],
                        oem: [40, 40],
                        ubea_version: 0,
                        status_1: 0,
                        status_2: 0,
                        esta_code: 123,
                        short_name: [
                            b'a', b'r', b't', b'n', b'e', b't', b'-', b'h', b'u', b'b', 0, 0, 0, 0,
                            0, 0, 0, 0,
                        ],
                        long_name: [
                            b'a', b'r', b't', b'n', b'e', b't', b'-', b'h', b'u', b'b', 0, 0, 0, 0,
                            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                            0, 0,
                        ],
                        node_report: [
                            b'a', b'r', b't', b'n', b'e', b't', b'-', b'h', b'u', b'b', 0, 0, 0, 0,
                            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                            0, 0,
                        ],
                        num_ports: [255, 255],
                        port_types: [0, 1, 2, 3],
                        good_input: [0, 1, 2, 3],
                        good_output: [0, 0, 0, 0],
                        swin: [0, 0, 0, 0],
                        sw_video: 0,
                        swout: [0, 0, 0, 0],
                        sw_macro: 0,
                        sw_remote: 0,
                        spare: [0, 0, 0],
                        style: 0,
                        mac: [1, 2, 3, 4, 5, 6],
                        bind_ip: [192, 168, 50, 187],
                        bind_index: 0,
                        filler: [
                            1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4,
                            5, 6,
                        ],
                    }));
                    let bytes = command.write_to_buffer().unwrap();

                    socket.send_to(&bytes, &addr).unwrap();
                }

                /*
                ArtCommand::PollReply(reply) => {
                    // Currently there is no reason to react to PollReply messages, since we are just
                    // getting data in

                    // This is an ArtNet node on the network. We can send commands to it like this:
                    let command = ArtCommand::Output(Output {
                        data: vec![1, 2, 3, 4, 5].into(),
                        ..Output::default()
                    });
                    let bytes = command.write_to_buffer().unwrap();
                    socket.send_to(&bytes, &addr).unwrap();

                }
                */
                _ => {
                    println!("Received unhandled {:?}", command);
                }
            }
        }
    } // the socket is closed here

    outputs.close();
}
