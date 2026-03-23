# Firmware

## Overview

Demetra's firmware is written in Rust, targeting the ESP32-S3 microcontroller. It uses the ESP-HAL ecosystem with the Embassy async runtime, and a Slint-based GUI for the touchscreen display.

The firmware source lives in the `firmware/` directory of the repository, organized as a Cargo workspace:

- **`chip/`** — the main ESP32-S3 application that runs on hardware
- **`lib/`** — shared business logic and UI components, testable on a host machine
- **`simulation/`** — a desktop simulation of the UI (no hardware required)

## Try the UI (No Toolchain Required)

If you just want to see the interface, you can run the simulation in Docker:

```bash
cd firmware
docker build -f Dockerfile.sim -t demetra-sim .
docker run --rm -p 6080:6080 demetra-sim
```

Then open [http://localhost:6080/vnc.html](http://localhost:6080/vnc.html) in your browser.

## Toolchain Setup

To build and flash the firmware onto an ESP32-S3, you'll need the Rust ESP toolchain.

### 1. Install Rust

If you don't already have Rust installed, you can go to the [rust-lang installation instructions](https://rust-lang.org/tools/install/) and follow the installation instructions.

This project also uses the standard nightly toolchain for host-side builds (tests, simulation), you can install it :

```bash
rustup install nightly
rustup component add rust-src --toolchain nightly
```

### 2. Install the ESP Toolchain

`espup` manages rust's the `esp` toolchain, which includes the custom LLVM backend needed for the ESP32-S3's Xtensa architecture.
To set that up, head to [the espup repository](https://github.com/esp-rs/espup) and follow the installation instructions.

### 3. Install espflash

[espflash](https://github.com/esp-rs/espflash/blob/main/espflash/README.md) is used to flash firmware onto the board and open a serial monitor.


## Building & Flashing

Connect the ESP32-S3 board via USB (J52) or a UART converter (J51), then, from inside the project's root directory, run:

The USB connection does _not_ supply power to the board, so if you're flashing using a USB cable, you'll need to plug in the power supply as well.

```bash
cd firmware
cargo +esp flash-chip
```

This builds the firmware in release mode and flashes it to the connected board.

### Other Useful Commands

```bash
# Build without flashing
cargo +esp build-chip-release

# Check for compilation errors (faster than a full build)
cargo +esp check-chip

# Run unit tests (host machine, no hardware needed)
cargo unit-test

# Run the desktop simulation outside of Docker (this will just pop up a window with the simulated UI)
cargo sim
```

## After Flashing

Once the firmware is flashed, head over to [Setup](build_guide/setup.md) for initial configuration, sensor calibration, and pump setup.
