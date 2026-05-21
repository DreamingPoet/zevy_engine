# zevy_engine

`zevy_engine` is a custom VR rendering engine project built with Rust, [Bevy](https://bevyengine.org/), and OpenXR.

The goal of this project is to support the production of real-time, first-person, interactive VR experiences with modern rendering features such as:

- PBR materials
- Multiple light sources
- OpenXR support
- Vulkan-based rendering

## Development Environment

This project uses the stable Rust toolchain.

Set the local toolchain to stable:

```bash
rustup override set stable
```

Check the installed Cargo version:

```bash
cargo --version
```

Inspect the currently installed Rust toolchains and check for updates:

```bash
rustup show
rustup check
```

## Project Structure

This repository is intended to focus on engine development first.

The overall development approach is:

- `zevy_engine` contains the engine-side code.
- A separate demo or content package will be developed alongside the engine.
- Engine code and demo/content code should remain clearly separated.

## Current Status

The current prototype already includes:

- Bevy integration
- OpenXR plugin setup
- A simple 3D scene for initial rendering validation

## Running the Project

To build and run the current prototype:

```bash
cargo run
```

## Development Goals

The planned work is divided into two tracks:

1. Engine development
   - Complete the Bevy + OpenXR integration
   - Continue expanding the rendering and VR feature set
2. Content development
   - Build interactive demo content on top of the engine
