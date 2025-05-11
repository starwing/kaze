# Kaze

Kaze is a Service Mesh framework designed to simplify microservice communication and management. It leverages shared memory channels for high-performance, low-latency communication between its sidecar proxy and the host application. `kaze` aims to provide robust service discovery, logging, monitoring, and RPC capabilities with minimal overhead.

## Core Concepts

- **Sidecar Proxy:** The `kaze run` command deploys a lightweight, high-performance sidecar proxy alongside your application.
- **Shared Memory Channels:** Kaze utilizes `kaze-core` to establish efficient communication channels via shared memory. This allows for near zero-copy message passing between the sidecar and the host application, significantly reducing inter-process communication (IPC) overhead.
- **Host Application Integration:** The Kaze sidecar, initiated by `kaze run`, creates these shared memory channels and then launches the host program. It manages network traffic and provides mesh capabilities to the co-located host application.

## Features

- **Service Discovery:** Dynamically discover and connect to other services within the mesh.
- **Enhanced Logging:** Provides capabilities for structured and centralized logging from your microservices.
- **Application Monitoring:** Collects metrics and facilitates monitoring of the health and performance of your services.
- **RPC Facilitation:** Simplifies and potentially offloads aspects of Remote Procedure Calls between services.
- **High-Performance IPC:** Utilizes shared memory channels via `kaze-core` for minimal communication overhead between the application and the sidecar.
- **Transparent Sidecar:** Aims to integrate with applications with minimal to no code changes in the host application.

## Installation

Details on installing Kaze:

```bash
cargo install kaze
```

## Usage

To run your application with Kaze, use the `kaze run` command:

```bash
kaze run <host_command_and_args>
```

This will start the Kaze sidecar and then launch your application, enabling Kaze's features for it.

## Project Components

Kaze is composed of several key components:

- **`kaze-core`**: Provides the foundational shared memory channel primitives for inter-process communication.
- **`kaze-edge`**: Acts as the communication bridge between the Kaze sidecar and the host application.
- **`kaze-host`**: Offers necessary utilities for integrating Rust-based host applications with Kaze.
- **`kaze-plugin`**: A framework for developing extensions and plugins to enhance sidecar functionality.
- **`kaze-protocol`**: Defines the basic communication protocol used between the sidecar and the host application.
- **`kaze-resolver`**: A framework for service discovery, enabling applications to find and connect to other services in the mesh.
- **`kaze-service`**: A middleware framework, serving as a preliminary implementation based on concepts from Tower 1.0.
- **`kaze-sidecar`**: Contains the main implementation code for the Kaze sidecar proxy.
