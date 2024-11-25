ARG RUST_VERSION=1.80.0

FROM public.ecr.aws/docker/library/rust:${RUST_VERSION} AS builder
WORKDIR /app/walnut-server
COPY . .
RUN make deps
ENV SQLX_OFFLINE=true
RUN cargo build --locked --release --bin server

# Final Stage
FROM alpine:3.14

# Install runtime dependencies
RUN apk add --no-cache \
    gcompat \
    libgcc \
    libstdc++ \
    ca-certificates \
    tzdata \
    bash \
    openssl

# Create non-root user
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/home" \
    --shell "/sbin/nologin" \
    --uid "10001" \
    appuser

RUN chown -R appuser /home

# Copy binaries from the builder stage
COPY --from=builder /app/walnut-server/target/release/server /opt/app/server
COPY --from=builder /app/walnut-server/universal-sierra-compiler /opt/app/universal-sierra-compiler
COPY --from=builder /app/walnut-server/binaries /opt/app/binaries
RUN chown -R appuser /opt/app

USER appuser
WORKDIR /opt/app
EXPOSE 3000
ENTRYPOINT ["/opt/app/server"]