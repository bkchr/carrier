FROM rust:1.24

RUN apt-get update
RUN apt install -y clang libssl-dev

WORKDIR /usr/src/carrier
COPY . .
RUN cargo install
CMD ["carrier-server"]