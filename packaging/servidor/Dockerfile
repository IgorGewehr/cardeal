# syntax=docker/dockerfile:1.7
# cardeal-server — o Cardeal online (ADR-0016): servidor + cliente do navegador numa imagem.
#
#   docker build -f packaging/servidor/Dockerfile -t cardeal-server .
#
# Imagem final: binário único + o app web pré-comprimido + glibc (distroless), sem shell,
# sem gerenciador de pacotes, rodando como usuário sem privilégio. TLS termina no Cloudflare
# Tunnel, não aqui.

# Ferramentas do build web, pré-compiladas (compilar o binaryen levaria ~10 min).
FROM rust:1.98-slim-bookworm AS ferramentas
# A CLI do wasm-bindgen tem de ser a mesma versão do Cargo.lock.
COPY Cargo.lock /tmp/Cargo.lock
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
 && curl -L --proto '=https' --tlsv1.2 -sSf \
      https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
 && versao=$(grep -A1 '^name = "wasm-bindgen"$' /tmp/Cargo.lock | sed -n 's/version = "\(.*\)"/\1/p') \
 && cargo binstall -y --locked "wasm-bindgen-cli@${versao}" wasm-opt

FROM rust:1.98-slim-bookworm AS construcao
COPY --from=ferramentas /usr/local/cargo/bin/wasm-bindgen /usr/local/cargo/bin/wasm-opt /usr/local/cargo/bin/
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p cardeal-server \
 && cp target/release/cardeal-server /cardeal-server \
 && cargo run --release --locked -p xtask -- construir-web \
 && cp -r dist/web /web \
 && mkdir /dados

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=construcao /cardeal-server /usr/local/bin/cardeal-server
COPY --from=construcao /web /web
# Volume novo herda o dono da pasta na imagem: o usuário sem privilégio (65532) consegue gravar.
COPY --from=construcao --chown=65532:65532 /dados /dados

# A primeira arena do mimalloc nasce antes do `main`: só o ambiente alcança esta opção
# (ver crates/cardeal-server/src/alocador.rs — metade da RAM com 200 empresas abertas).
ENV MIMALLOC_ARENA_EAGER_COMMIT=0 \
    CARDEAL_DADOS=/dados \
    CARDEAL_WEB=/web \
    CARDEAL_ENDERECO=0.0.0.0:8080 \
    RUST_LOG=info,tower_http=warn

VOLUME ["/dados"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/cardeal-server"]
CMD ["servir"]
