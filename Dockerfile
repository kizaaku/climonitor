
# Use a Rust base image
FROM rust:1.78-buster AS builder

# Set the working directory
WORKDIR /app

# Copy the entire project
COPY . .

# Build the workspace
RUN cargo build --release

# Use a minimal base image for the final stage
FROM debian:buster-slim

# Copy the built binaries from the builder stage
COPY --from=builder /app/target/release/ccmonitor-launcher /usr/local/bin/
COPY --from=builder /app/target/release/ccmonitor /usr/local/bin/

# Set the entrypoint for the launcher (this can be overridden in docker-compose.yml)
ENTRYPOINT ["ccmonitor-launcher"]
