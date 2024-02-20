# Stage 1: Build the binary
FROM rust:latest as builder

# Copy the sources over
WORKDIR /hotstuff
COPY . .

# Build the project
RUN cargo build --release

# Stage 2: Setup the runtime environment
FROM debian:bookworm

# Copy the binary from the builder stage
COPY --from=builder /hotstuff/target/release/hotstuff /usr/local/bin/hotstuff

# Startup command runs the node
CMD ["hotstuff"]
