# icecc-mon

A TUI monitor for the `icecc` distributed compilation system.

`icecc-mon` provides a terminal-based user interface to monitor the status of distributed compilation tasks managed by `icecc`. It allows users to visualize the progress of compilation across different nodes in the `icecc` network.

## Features

- Real-time monitoring of `icecc` distributed compilation.
- Terminal User Interface (TUI) powered by `ratatui`.
- Asynchronous communication with `icecc` nodes using `tokio` and a custom `icecc-protocol`.

## Getting Started

### Prerequisites

- [Rust and Cargo](https://rustup.rs/)

### Installation

Clone the repository and build it with Cargo:

```bash
cargo build --release
```

The binary will be located in `target/release/icecc-mon`.

## License

This project is licensed under the GPL-v2.
