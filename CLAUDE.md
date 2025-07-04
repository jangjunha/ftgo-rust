# FTGO Rust Project Commands & Guidelines

## Build & Run Commands
- Build: `cargo build`
- Release build: `cargo build --release`
- Build specific service: `cargo build -p ftgo-<service-name>-service`
- Build specific binary: `cargo build -p ftgo-<service-name>-service --bin <binary>`

## Test Commands
- Run all tests: `cargo test`
- Run specific test: `cargo test <test_name>`
- Test specific service: `cargo test -p ftgo-<service-name>-service`

## Lint & Format
- Format code: `cargo fmt`
- Lint code: `cargo clippy`
- Check code: `cargo check`

## Code Style Guidelines
- **Naming**: snake_case for functions/variables, CamelCase for types/traits
- **Error Handling**: Use thiserror for custom error types, ? operator for propagation
- **Modules**: Each service has separate binaries for RPC, consumers, producers
- **Imports**: Group standard lib, external crates, then internal modules
- **Comments**: Document public APIs with /// comments

## Architecture
- Microservice architecture with gRPC (tonic) communication
- PostgreSQL with Diesel ORM for data persistence
- Event-driven design with outbox pattern for consistency
