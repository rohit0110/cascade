# Builds the UI, then the Rust binaries (UI embedded), into a small non-root image.
FROM node:22-bookworm-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY --from=web /web/dist web/dist
RUN cargo build --release --locked -p cascade-server -p cascade-cli

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/cascade-server /usr/local/bin/cascade-server
COPY --from=build /src/target/release/cascade /usr/local/bin/cascade
USER nonroot
EXPOSE 7070
ENTRYPOINT ["/usr/local/bin/cascade-server"]
