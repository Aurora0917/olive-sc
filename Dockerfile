# # Use official Ubuntu 22.04 as the base image
# FROM ubuntu:22.04

# # Set environment variables to avoid interactive prompts
# ENV DEBIAN_FRONTEND=noninteractive

# # Install dependencies
# RUN apt-get update && apt-get install -y \
#     curl \
#     build-essential \
#     pkg-config \
#     libssl-dev \
#     git \
#     && rm -rf /var/lib/apt/lists/*

# # Install Rust (required for Solana smart contract development)
# RUN curl --proto '=https' --tlsv1.2 -sSfL https://raw.githubusercontent.com/solana-developers/solana-install/main/install.sh | bash

FROM rust:1.84-slim

# Set environment variables for non-interactive installations
ENV DEBIAN_FRONTEND=noninteractive

# Install necessary tools
RUN apt-get update && apt-get install -y \
    wget \
    tar \
    bzip2 \
    nodejs \
    npm \
    yarnpkg \
    git \
    --no-install-recommends && rm -rf /var/lib/apt/lists/*

# debian calls yarn "yarnpkg" by default
RUN ln -s /usr/bin/yarnpkg /usr/bin/yarn

# Set working directory
WORKDIR /opt

# Download and install the Solana release
RUN wget -q https://github.com/anza-xyz/agave/releases/download/v2.1.15/solana-release-x86_64-unknown-linux-gnu.tar.bz2 \
    && tar -xvjf solana-release-x86_64-unknown-linux-gnu.tar.bz2 \
    && rm solana-release-x86_64-unknown-linux-gnu.tar.bz2

# Add the Solana binaries to the PATH
ENV PATH="/opt/solana-release/bin:$PATH"

# Verify the installation
RUN solana --version || echo "Installation failed"

# Install anchor
RUN cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli

# Permit user to use cargo
RUN chmod -R go+rwX /usr/local/cargo/
# Permit user to install platform-tools
RUN chmod -R go+rwX /opt/solana-release/bin/sdk/


# Set working directory
WORKDIR /workspace

# Default command
CMD ["solana", "--help"]