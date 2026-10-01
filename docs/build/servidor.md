# Servidor online — implantação

O `cardeal-server` (ADR-0016) roda numa VPS pequena, atrás do Cloudflare Tunnel. Este guia
cobre a imagem, o provisionamento de empresas e a operação.

## 1. Arquitetura

```
navegador / desktop ─ HTTPS ─ Cloudflare (TLS, WAF, DDoS)
                               │  túnel de saída (nenhuma porta aberta na VPS)
                     VPS: cloudflared ─► cardeal-server :8080
                                          ├─ /dados/diretorio.db
                                          └─ /dados/empresas/<id>.db  (1 por empresa)
```

## 2. Números medidos

`crates/cardeal-server/examples/carga.rs`, 200 empresas, notebook de desenvolvimento,
2026-10-01:

| | |
|---|---|
| Imagem Docker | 36 MB (servidor + cliente web pré-comprimido + glibc distroless) |
| Container em repouso | ~2,5 MiB |
| Container depois de login + consulta | ~4,4 MiB |
| Por empresa aberta | ~0,8 MB de heap (0,53 MB dentro do SQLite) |
| Empresa fria (1ª requisição após ociosa) | p50 2,0 ms (impressão do plano: pula verificação) |
| Consulta quente, ponta a ponta | p50 70 µs, p99 80 µs |
| Cliente web | WASM 2,33 MB → **1,03 MB brotli**; aba ~4 MB acima de uma aba vazia |

Para reproduzir:

```sh
MIMALLOC_ARENA_EAGER_COMMIT=0 cargo run --release -p cardeal-server --example carga -- 200
```

## 3. Subir

```sh
export CLOUDFLARE_TUNNEL_TOKEN=...      # painel Zero Trust → Tunnels → token
docker compose -f packaging/servidor/compose.yaml up -d --build
```

No painel do túnel, aponte o hostname público (ex.: `app.cardeal.com.br`) para
`http://cardeal:8080`.

## 4. Cliente do navegador

```sh
cargo binstall wasm-bindgen-cli@0.2.127 wasm-opt   # uma vez
cargo xtask construir-web                          # → dist/web/
cardeal-server servir --web dist/web               # ou CARDEAL_WEB=dist/web
```

O mesmo `cardeal-ui` do desktop, compilado para WASM (egui + WebGL2). O `index.html` vai
sem cache; o resto mora em `app/<hash do conteúdo>/` com cache imutável de um ano e
`.br`/`.gz` gerados no build — o servidor não comprime nada por requisição. A página tem CSP
estrita (`script-src 'self' 'wasm-unsafe-eval'`, nada inline). Medido (2026-10-01, Firefox
156): WASM de 3,75 MB / **1,37 MB em brotli**; segunda abertura em ~0,3 s; a aba do app tem
~9 MB privados a mais que uma aba vazia (sem contar a composição gráfica, que vai para a GPU).

## 5. Criar uma empresa

```sh
CARDEAL_SENHA='senha-da-conta' docker compose -f packaging/servidor/compose.yaml run --rm \
  -e CARDEAL_SENHA cardeal provisionar \
  --empresa "Assistência Exemplo" --cnpj 11222333000181 \
  --email dono@exemplo.com --nome "Dono"
```

A imagem já traz o cliente do navegador (`CARDEAL_WEB=/web`): abra o hostname do túnel e
entre com o e-mail e a senha da conta.

A senha vai por variável de ambiente, nunca por argumento (argumento aparece no histórico do
shell e no `ps`). Um e-mail que já tem conta ganha acesso à empresa nova sem trocar a senha —
é assim que um contador atende várias empresas com um login só.

## 6. Variáveis

| Variável | Padrão | O quê |
|---|---|---|
| `CARDEAL_DADOS` | `/dados` | diretório + bases |
| `CARDEAL_ENDERECO` | `0.0.0.0:8080` | escuta |
| `CARDEAL_OCIOSIDADE_MIN` | `10` | minutos sem uso até a empresa sair da memória |
| `CARDEAL_TETO_EMPRESAS` | `256` | máximo aberto ao mesmo tempo |
| `CARDEAL_TRABALHADORES` | `2` | threads do runtime (o trabalho de banco vai ao pool de bloqueio) |
| `CARDEAL_WEB` | — | pasta do cliente do navegador (`dist/web`) |
| `CARDEAL_CONFIAR_CLOUDFLARE` | `false` | usar `CF-Connecting-IP`; **só** com a porta inacessível de fora do túnel |
| `MIMALLOC_ARENA_EAGER_COMMIT` | `0` (na imagem) | não reservar memória antes do uso |
| `CARDEAL_BACKUP_MIN` | `60` | minutos entre backups (0 desliga) |
| `CARDEAL_BACKUP_RETER` | `48` | cópias guardadas por base |
| `RUST_LOG` | `info,tower_http=warn` | `debug` registra toda requisição |

## 7. Segurança

- Senhas: Argon2id (19 MiB, t=2), no máximo 2 hashes simultâneos (pico de RAM fixo).
- Força bruta: bloqueio progressivo por conta (5 → 1 min, 10 → 15 min) **e** 30 tentativas
  por IP a cada 5 min (HTTP 429).
- Sessão: token de 32 bytes; o servidor guarda só o BLAKE3. Navegador recebe cookie
  `HttpOnly; Secure; SameSite=Strict`; o desktop, `Authorization: Bearer`.
- CSRF: `SameSite=Strict` + cabeçalho obrigatório `x-cardeal-protocolo` (força preflight).
- Respostas da API: `Cache-Control: no-store`, CSP `default-src 'none'`, HSTS, `nosniff`.
- Falha interna nunca vaza SQL ou caminho: o cliente recebe uma referência, o log tem o resto.
- Container: usuário sem privilégio, sistema de arquivos somente leitura (exceto `/dados`),
  `no-new-privileges`.

## 8. Operação

- **Desligar:** `SIGTERM` (o padrão de `docker stop`) termina as requisições em curso e fecha
  cada base com o lote final confirmado.
- **Manutenção** (a cada minuto, sozinha): fecha empresas ociosas, poda respostas idempotentes
  com mais de 7 dias, apaga sessões vencidas.
- **Backup:** o servidor tira, a cada `CARDEAL_BACKUP_MIN` (60), um snapshot de cada base
  **que mudou** (`VACUUM INTO` numa conexão de leitura — a empresa continua operando), em
  `/dados/backup/<base>/<carimbo UTC>.db.zst`, guardando as `CARDEAL_BACKUP_RETER` (48) mais
  novas. O serviço `backup-r2` do compose copia a pasta para o Cloudflare R2 **cifrada no
  cliente** (`rclone crypt`). Variáveis: `R2_ENDPOINT`, `R2_ACCESS_KEY_ID`,
  `R2_SECRET_ACCESS_KEY`, `R2_BUCKET`, `R2_CRIPTO_SENHA` (`rclone obscure`).
  Rodada manual: `cardeal-server backup`.
- **Restaurar uma empresa:** parar o servidor (ou esperar a empresa sair da memória),
  `zstd -d <cópia>.db.zst -o /dados/empresas/<id>.db`, apagar `<id>.db-wal`/`-shm` antigos,
  subir de novo. A cópia passa por `PRAGMA integrity_check` no teste de ponta a ponta.
