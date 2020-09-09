FROM rust
WORKDIR /usr/app

RUN apt-get update
RUN apt-get install clang -y

RUN USER=root cargo new --bin sfshr-server
WORKDIR sfshr-server

COPY Cargo.toml .
RUN cargo build --release
RUN rm -f target/release/deps/sfshr_server*

COPY . .
RUN cargo build --release
EXPOSE 40788

CMD ["cargo", "run", "--release"]
