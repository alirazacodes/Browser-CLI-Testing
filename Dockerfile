FROM rust:latest

WORKDIR /app

# Install dependencies
RUN apt-get update && \
    apt-get install -y libpq-dev ca-certificates curl sudo && \
    rm -rf /var/lib/apt/lists/*

COPY . .

# Build app
RUN cargo build --release

# Create directory
RUN mkdir -p /app/data

# Expose port
EXPOSE 8080

# Entrypoint
CMD ["/app/target/release/lavachallenge"] 