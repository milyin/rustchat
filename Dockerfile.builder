FROM rust:latest
WORKDIR /tmp
ADD . .
RUN cargo fetch


