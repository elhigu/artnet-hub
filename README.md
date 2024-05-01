# Art-Net Hub

Art-Net proxy server, which listens art-net packages and re-routes universes to other
hosts with rate limiter and optional custom protocols for delivering universes to final device.

The original use case for this application is to operate on a separate mini-server, which is connected via Ethernet to the VJ network. Its purpose is to function as a device to which other applications, like TD and
Resolume sends the Art-Net data. The device also acts as a WiFi access point for ESP32 devices to connect wirelessly. This configuration enables the application to efficiently receive Art-Net data and route it to multiple ESP32 devices with Art-Net or with any custom protocol.

With custom protocol ESP32 can for example send all universes of a single frame as a single UPD
packet. TODO: add benchmark info, how many universes it can deliver per second

## Getting started

    cargo build
    cargo run

## Configuration

    {
        "listen": {
            "//": "When running locally e.g. with Resolume put network interface's address here and set lumiverse to send data that IP",
            "address": "0.0.0.0",
            "port": 6454
        },
        "mappings": [
            {
            "host": { "address": "192.168.0.11", "port": 6454 },
            "//": "Mapping which universes will be realayed here and what how they are mapped",
            "universes": [{ "input": [16, 31], "output_start": 0 }]
            }
        ]
    }

## TODO:

- Art-Net server protocol to receive artnet data and broadcast available devices
- UDP threads for each device to pass universes to final controllers.
