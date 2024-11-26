FROM rust:1.82.0-bookworm AS builder

WORKDIR /app/walnut-server
COPY . .
RUN make deps
ENV SQLX_OFFLINE=true
RUN cargo build --locked --release --bin server

FROM rust:1.82.0-bookworm AS final

WORKDIR /opt/app

COPY --from=builder /app/walnut-server/target/release/server /opt/app/server
COPY --from=builder /app/walnut-server/universal-sierra-compiler /opt/app/universal-sierra-compiler

RUN mkdir -p /opt/app/binaries
RUN chmod +rwx -R /opt/app/binaries

EXPOSE 3000

ENTRYPOINT ["/opt/app/server"]
