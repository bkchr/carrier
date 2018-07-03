FROM rust:latest

RUN apt-get update
RUN apt install -y clang libssl-dev

WORKDIR /usr/src/carrier
COPY . .
RUN cargo install
CMD ["carrier-bearer"]
