FROM rust:1.82.0-bookworm AS builder
WORKDIR /app/walnut-server
COPY . .
RUN make deps
ENV SQLX_OFFLINE=true
RUN cargo build --locked --release --bin server

FROM rust:1.82.0-bookworm AS final
RUN adduser \
  --disabled-password \
  --gecos "" \
  --home "/home" \
  --shell "/sbin/nologin" \
  --uid "10001" \
  appuser

RUN chown -R appuser /home
COPY --from=builder /app/walnut-server/target/release/server /opt/app/server
COPY --from=builder /app/walnut-server/universal-sierra-compiler /opt/app/universal-sierra-compiler
COPY --from=builder /app/walnut-server/binaries /opt/app/binaries
RUN chown -R appuser /opt/app
USER appuser
WORKDIR /opt/app
EXPOSE 3000
ENTRYPOINT ["/opt/app/server"]