# syntax=docker/dockerfile:1.7
# cardeal-server — o Cardeal online (ADR-0016).
#
#   docker build -f packaging/servidor/Dockerfile -t cardeal-server .
#
# Imagem final: binário único + glibc (distroless), sem shell, sem gerenciador de pacotes,
# rodando como usuário sem privilégio. TLS termina no Cloudflare Tunnel, não aqui.

FROM rust:1.98-slim-bookworm AS construcao
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p cardeal-server \
 && cp target/release/cardeal-server /cardeal-server \
 && mkdir /dados

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=construcao /cardeal-server /usr/local/bin/cardeal-server
# Volume novo herda o dono da pasta na imagem: o usuário sem privilégio (65532) consegue gravar.
COPY --from=construcao --chown=65532:65532 /dados /dados

# A primeira arena do mimalloc nasce antes do `main`: só o ambiente alcança esta opção
# (ver crates/cardeal-server/src/alocador.rs — metade da RAM com 200 empresas abertas).
ENV MIMALLOC_ARENA_EAGER_COMMIT=0 \
    CARDEAL_DADOS=/dados \
    CARDEAL_ENDERECO=0.0.0.0:8080 \
    RUST_LOG=info,tower_http=warn

VOLUME ["/dados"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/cardeal-server"]
CMD ["servir"]
