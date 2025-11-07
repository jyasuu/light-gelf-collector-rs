# Stage 1: Build
FROM rust:1.90.0 AS builder

# Set the working directory
WORKDIR /usr/src/app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY . .

# Build the project in release mode
RUN cargo build --release

# Stage 2: Runtime
FROM ubuntu:24.04

# Set the locale environment variables
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US:en
ENV LC_ALL=en_US.UTF-8

# Install dependencies and set up UTF-8 locale
RUN apt-get update && \
    apt-get install -y --no-install-recommends locales libssl-dev ca-certificates && \
    echo "en_US.UTF-8 UTF-8" >> /etc/locale.gen && \
    locale-gen en_US.UTF-8 && \
    rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app

# Copy the compiled binary from the build stage
COPY --from=builder /usr/src/app/target/release/light-gelf-collector-rs .

EXPOSE 8080 12201/udp

# Set the startup command
CMD ["./light-gelf-collector-rs"]
