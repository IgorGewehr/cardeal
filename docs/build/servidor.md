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
| Imagem Docker | 31 MB (binário + glibc distroless) |
| Container em repouso | ~2 MiB |
| Por empresa aberta | ~0,8 MB de heap (0,53 MB dentro do SQLite) |
| Empresa fria (1ª requisição após ociosa) | p50 11 ms |
| Consulta quente, ponta a ponta | p50 120 µs, p99 210 µs |

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

## 4. Criar uma empresa

```sh
CARDEAL_SENHA='senha-da-conta' docker compose -f packaging/servidor/compose.yaml run --rm \
  -e CARDEAL_SENHA cardeal provisionar \
  --empresa "Assistência Exemplo" --cnpj 11222333000181 \
  --email dono@exemplo.com --nome "Dono"
```

A senha vai por variável de ambiente, nunca por argumento (argumento aparece no histórico do
shell e no `ps`). Um e-mail que já tem conta ganha acesso à empresa nova sem trocar a senha —
é assim que um contador atende várias empresas com um login só.

## 5. Variáveis

| Variável | Padrão | O quê |
|---|---|---|
| `CARDEAL_DADOS` | `/dados` | diretório + bases |
| `CARDEAL_ENDERECO` | `0.0.0.0:8080` | escuta |
| `CARDEAL_OCIOSIDADE_MIN` | `10` | minutos sem uso até a empresa sair da memória |
| `CARDEAL_TETO_EMPRESAS` | `256` | máximo aberto ao mesmo tempo |
| `CARDEAL_TRABALHADORES` | `2` | threads do runtime (o trabalho de banco vai ao pool de bloqueio) |
| `CARDEAL_CONFIAR_CLOUDFLARE` | `false` | usar `CF-Connecting-IP`; **só** com a porta inacessível de fora do túnel |
| `MIMALLOC_ARENA_EAGER_COMMIT` | `0` (na imagem) | não reservar memória antes do uso |
| `RUST_LOG` | `info,tower_http=warn` | `debug` registra toda requisição |

## 6. Segurança

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

## 7. Operação

- **Desligar:** `SIGTERM` (o padrão de `docker stop`) termina as requisições em curso e fecha
  cada base com o lote final confirmado.
- **Manutenção** (a cada minuto, sozinha): fecha empresas ociosas, poda respostas idempotentes
  com mais de 7 dias, apaga sessões vencidas.
- **Backup:** pendente — Litestream replicando `/dados` para o Cloudflare R2 (ver
  `docs/19-estado-e-processo.md`).
