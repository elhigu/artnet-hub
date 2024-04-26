use serde::{Deserialize, Serialize};
use std::io::Read;
use std::{fs::File, str::FromStr};

#[derive(Serialize, Deserialize, Debug)]
struct AddressConfig {
    address: String,
    port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct UniverseMappingConfig {
    input: (u16, u16),
    output: (u16, u16),
}

#[derive(Serialize, Deserialize, Debug)]
struct DeviceMappingConfig {
    host: AddressConfig,
    universes: Vec<UniverseMappingConfig>,
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

// TODO: create Device mapping output threads

use artnet_protocol::*;
use std::net::{ToSocketAddrs, UdpSocket};

fn main() {
    let config = read_config_file("config.json").unwrap();

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

            println!("Received {:?}", command);

            match command {
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

                ArtCommand::PollReply(reply) => {
                    /* Currently there is no reason to react to PollReply messages, since we are just getting data in

                    // This is an ArtNet node on the network. We can send commands to it like this:
                    let command = ArtCommand::Output(Output {
                        data: vec![1, 2, 3, 4, 5].into(),
                        ..Output::default()
                    });
                    let bytes = command.write_to_buffer().unwrap();
                    socket.send_to(&bytes, &addr).unwrap();
                    */
                }
                _ => {}
            }
        }
    } // the socket is closed here
}
