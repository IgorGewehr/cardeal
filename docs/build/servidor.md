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

Imagem pronta (publicada pela CI em cada tag `v*.*.*`, workflow `servidor`):

```sh
export CARDEAL_IMAGEM=ghcr.io/<dono>/cardeal-server:<versão>
export CLOUDFLARE_TUNNEL_TOKEN=...
docker compose -f packaging/servidor/compose.yaml up -d
```

Ou construída na própria máquina:

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
| `CARDEAL_METRICAS_TOKEN` | — | liga `GET /metricas` (Prometheus) com `Authorization: Bearer` |
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
- O cliente do navegador exige HTTPS (o cookie é `Secure`): use o túnel, não `http://` na LAN.
- Gerenciar equipe (`POST`/`DELETE /v1/e/{empresa}/membros`) e trocar senha
  (`POST /v1/sessao/senha`, derruba as outras sessões) conferem permissão antes de tocar
  em qualquer base.
- Container: usuário sem privilégio, sistema de arquivos somente leitura (exceto `/dados`),
  `no-new-privileges`.

## 8. Operação

- **Saúde:** `HEALTHCHECK` da imagem roda `cardeal-server saude` (o próprio binário faz o
  `GET /saude` — a imagem não tem curl nem shell); o túnel só sobe com o servidor saudável.
- **Desligar:** `SIGTERM` (o padrão de `docker stop`) termina as requisições em curso e fecha
  cada base com o lote final confirmado.
- **Manutenção** (a cada minuto, sozinha): fecha empresas ociosas, poda respostas idempotentes
  com mais de 7 dias, apaga sessões vencidas.
- **Métricas:** `GET /metricas` (com `CARDEAL_METRICAS_TOKEN`): requisições e erros por
  tipo (comando, consulta, sessão, estático), histograma de latência, logins recusados,
  bloqueios por IP, empresas abertas, sessões em cache e RSS do processo. Custo no caminho
  quente: dois incrementos atômicos.
- **Replicação contínua (perda máxima = último commit):** cada base (empresas e diretório)
  copia, depois de cada commit e **antes** de responder, os bytes novos do WAL para
  `/dados/replica/<base>/` (gerações = base exata + WAL de cada intervalo entre checkpoints;
  checkpoints controlados pelo replicador). Custo medido: ~18 µs por commit (<1%). O serviço
  `backup-r2` leva a réplica ao R2 cifrada a cada 10 s (perda máxima fora da máquina ≈ 15 s).
  Ligada por padrão.
- **Snapshots (segunda camada, histórico):** a cada `CARDEAL_BACKUP_MIN` (60), um
  `VACUUM INTO` de cada base que mudou, em `/dados/backup/<base>/<carimbo>.db.zst`, guardando
  as `CARDEAL_BACKUP_RETER` (48) mais novas — para voltar a um ponto anterior ("apaguei sem
  querer ontem"). Rodada manual: `cardeal-server backup`.
- **Trocar de servidor (disco morreu, VPS sumiu):**
  1. Na máquina nova: `rclone copy cifrado:replica /dados-novo/replica`.
  2. `cardeal-server --dados /dados-novo restaurar --de /dados-novo/replica` — reconstrói o
     diretório e cada empresa (contiguidade conferida, `integrity_check`); nunca sobrescreve.
  3. Subir o compose apontando para `/dados-novo` com o **mesmo** `CLOUDFLARE_TUNNEL_TOKEN`:
     o endereço público não muda. Ensaiado no teste
     `servidor_novo_restaurado_da_replica_continua_de_onde_o_velho_parou`.
- **Restaurar uma empresa:** parar o servidor (ou esperar a empresa sair da memória),
  `zstd -d <cópia>.db.zst -o /dados/empresas/<id>.db`, apagar `<id>.db-wal`/`-shm` antigos,
  subir de novo. A cópia passa por `PRAGMA integrity_check` no teste de ponta a ponta.
