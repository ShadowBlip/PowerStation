FROM rust:1.83

RUN apt-get update && apt-get install -y \
	cmake \
	build-essential \
	libpci-dev \
	libclang-15-dev
