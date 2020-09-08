FROM rust
WORKDIR /usr/app
COPY . .
RUN apt-get update
RUN apt-get install clang -y
RUN cargo build --release
EXPOSE 40788
CMD ["cargo", "run", "--release"]
