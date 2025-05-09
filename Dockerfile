FROM rust:1.86

RUN dpkg --add-architecture arm64
RUN apt-get update && apt-get install -y \
	cmake \
	build-essential \
	libpci-dev \
	libclang-15-dev

RUN apt-get install -y \
  g++-aarch64-linux-gnu \
  libc6-dev-arm64-cross \
  libpci-dev:arm64

RUN rustup target add aarch64-unknown-linux-gnu

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
ENV CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
ENV CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
