# icecc-mon

A TUI monitor for the `icecc` distributed compilation system.

`icecc-mon` provides a terminal-based user interface to monitor the status of distributed compilation tasks managed by `icecc`. It allows users to visualize the progress of compilation across different nodes in the `icecc` network.

Heavily inspired by `icecream-sundae`, `icecc-mon` is an extremely lightweight implementation written in Rust.

## Getting Started

### Prerequisites

- [Rust and Cargo](https://rustup.rs/)

## Installation

### From Source

Clone the repository and build it with Cargo:

```bash
cargo build --release
```

The binary will be located in `target/release/icecc-mon`.

### Homebrew

Install via Homebrew using the following tap:

```bash
brew tap timblechmann/icecc-mon-rs-tap
brew install icecc-mon
```

### Debian

Download the `.deb` package from the [releases page](https://github.com/timblechmann/icecc-mon-rs/releases) and install it using `apt`:

```bash
sudo apt install ./icecc-mon_*.deb
```

## License

This project is licensed under the GPL-v2.
