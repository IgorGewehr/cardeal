# ADR-0016 — Servidor multi-tenant com um SQLite por empresa e cliente web em egui/WASM

- **Status:** Aceita
- **Data:** 2026-10-01
- **Decisores:** Igor Gewehr
- **Relacionadas:** ADR-0003, ADR-0004, ADR-0008, ADR-0009, ADR-0012, ADR-0015

## Contexto

O Cardeal nasceu instalado: um processo, um arquivo `.db`, ~55 MB de RSS (ADR-0009). A pergunta
nova é oferecê-lo **online**, para muitas empresas (multi-tenant) e várias empresas por conta
(contador, grupo com filiais), com login por e-mail e senha — **sem** virar o que o mercado
reclama dos concorrentes: "a versão web é mais lenta e menos robusta que a instalada".

Restrições que não se negociam:

1. **RAM baixa no servidor.** Centenas de empresas pequenas numa VPS de 4 GB. O custo de
   infraestrutura por empresa tem que ficar abaixo de R$ 1/mês.
2. **RAM baixa no navegador.** Abaixo de 80 MB por aba, contra 200–400 MB de um SPA típico.
3. **Paridade com o nativo.** A web não pode ser uma "versão reduzida".
4. **Rapidez.** p99 de comando < 20 ms no servidor (excluída a rede).
5. **Nada do domínio muda.** Autorização, idempotência e despacho são os mesmos do desktop.

## Alternativas consideradas

### Onde moram os dados

| Critério | **Um SQLite por empresa** | Postgres compartilhado com `tenant_id` | Um Postgres/schema por empresa |
|---|---|---|---|
| Isolamento entre empresas | físico (arquivo) | lógico (`WHERE` que alguém esquece) | físico |
| RAM ociosa por empresa | **zero** (fechado quando ocioso) | conexões do pool | conexões + catálogo por schema |
| Reuso do código atual | **total** (ADR-0003/0012) | reescrever a persistência | reescrever a persistência |
| Backup/restauração/LGPD de uma empresa | copiar/apagar um arquivo | `DELETE` em N tabelas | `DROP SCHEMA` |
| Escala de escrita | um escritor por empresa — milhares de commits/s cada | um banco para todos | idem |
| Consultas entre empresas | não existem (e não deveriam) | fáceis | difíceis |

### Como o navegador desenha a tela

| Critério | **egui em WASM** | HTML no servidor (maud + Datastar) | Leptos/Dioxus (DOM em WASM) |
|---|---|---|---|
| Paridade com o desktop | **idêntica** — mesmas telas, mesmo `cardeal-ui` | sistema de UI paralelo | sistema de UI paralelo |
| ADR-0015 (um só sistema de componentes) | **mantido** | violado | violado |
| RAM na aba | 40–80 MB | 15–30 MB | 25–50 MB |
| Download inicial | ~1,5–2 MB (br), cacheado para sempre por hash | ~50 KB | ~0,5–1 MB |
| Texto selecionável / Ctrl+F / leitor de tela | fraco (canvas) | nativo | nativo |

A escolha pela **paridade** foi do usuário (2026-10-01), porque é exatamente a reclamação contra
os concorrentes. Os pontos fracos do canvas (seleção, acessibilidade, teclado móvel) ficam como
dívida conhecida, mitigada por `AccessKit` (o egui já o integra) e por um cliente móvel enxuto
posterior, se o uso pedir.

## Decisão

**Um binário `cardeal-server` (axum/tokio) serve muitas empresas. Cada empresa é um arquivo
SQLite próprio, aberto sob demanda e fechado quando ocioso. Contas (e-mail + senha) moram num
diretório global à parte. O cliente — desktop nativo ou egui compilado para WASM — fala com o
servidor pelo mesmo despacho por nome + `postcard` que o modo monoposto já usa.**

```
navegador (egui/WASM) ─┐                    ┌─ diretorio.db   contas, vínculos, sessões
desktop nativo ────────┼─ HTTPS ─ Cloudflare ─ VPS: cardeal-server ─┤
                       ┘   Tunnel           └─ empresas/<id>.db  1 por empresa (aberto sob demanda)
                                               └─ Litestream → R2  backup contínuo
```

### Peças

| Crate | Papel |
|---|---|
| `cardeal-motor` | O motor de **uma** empresa: abre/migra a base, autentica e despacha. Extraído de `cardeal-cliente` para que desktop e servidor compartilhem um só motor. |
| `cardeal-distribuicao` | A lista de módulos que este produto liga e o pedido de ativação padrão. É a única fonte, usada por desktop e servidor. |
| `cardeal-protocol` | Os tipos que trafegam na rede: rotas, cabeçalhos, pedidos de login/empresa. Sem tokio nem rusqlite, para compilar em WASM. |
| `cardeal-server` | O diretório, a frota de motores (abrir sob demanda / despejar ociosos), as sessões e as rotas HTTP. |

### Rotas (três verbos, doc 09 §1)

| Rota | O quê |
|---|---|
| `POST /v1/sessao` | login (e-mail + senha) → cookie de sessão + empresas acessíveis |
| `DELETE /v1/sessao` | logout |
| `POST /v1/e/{empresa}/cmd/{nome}` | comando; corpo = carga `postcard`; `Idempotency-Key` obrigatório |
| `POST /v1/e/{empresa}/qry/{nome}` | consulta; corpo = carga `postcard` |
| `GET /saude` | prontidão |

A resposta é a saída em `postcard` (200) ou um `cardeal_kernel::Erro` em `postcard` com o status
HTTP derivado da faixa do código (1xxx→422, 2xxx→409/404, 3xxx→401/403, 4xxx→409, 5xxx→503).
Não existe tipo de erro paralelo para a rede: é o mesmo `Erro` que a tela já sabe mostrar.

### Orçamento de memória

| Item | Orçamento | Como |
|---|---|---|
| Processo vazio | < 15 MB RSS | tokio com 2 workers, `mimalloc`, nada carregado no boot |
| Empresa aberta | < 4 MB | perfil `ConfigArmazenamento::servidor`: cache de 2 MB no escritor e 1 MB no leitor, um leitor só. O `mmap` usa o page cache do SO, que não conta como memória anônima e é reclamável. |
| Empresa ociosa | **0** | despejada após `ociosidade` (padrão 10 min) |
| Teto de empresas abertas | configurável (padrão 256) | ao atingir, despeja a menos usada |
| Login simultâneo | 2 × 19 MiB (Argon2id) | semáforo: o pico de RAM de hash não cresce com a carga |
| Sessão | ~200 B | token opaco de 32 bytes; só o BLAKE3 dele é guardado |

### Segurança

- **Cookie** `HttpOnly; Secure; SameSite=Strict` para o navegador: um script injetado não lê o
  token. O desktop usa o cabeçalho `Authorization: Bearer`, com o mesmo token.
- O token **nunca** é guardado em claro: o diretório guarda `BLAKE3(token)`.
- Bloqueio progressivo por conta (o mesmo `cardeal_auth::Bloqueio`), com erro genérico
  anti-enumeração.
- Autorização: a sessão prova a **conta**. O vínculo conta→empresa→usuário prova que ela entra
  naquela empresa. Os papéis do usuário **dentro** da empresa decidem cada comando, com o mesmo
  `cardeal_auth::autorizar` do desktop. Não existe caminho privilegiado.
- Idempotência: o `Despachante` grava a resposta em `nucleo_idempotencia` **na mesma transação**
  do comando. Uma repetição por queda de rede devolve a resposta original e não cria uma
  segunda venda.

### Medido na implementação (2026-10-01/02)

| Item | Orçamento | Medido |
|---|---|---|
| Processo vazio | < 15 MB | 6,9 MB (container em repouso: 2,5 MiB) |
| Empresa aberta | < 4 MB | ~0,8 MB de heap (0,53 MB dentro do SQLite) |
| Comando p99 (sem rede) | < 20 ms | consulta quente p50 70 µs / p99 80 µs |
| Empresa fria | — | 2,0 ms (impressão do plano pula verificações) |
| Aba do navegador | < 80 MB | ~4 MB acima de uma aba vazia (Firefox headless) |
| Download do cliente web | ~1,5–2 MB | 1,03 MB (brotli), fontes recortadas |

Medição reproduzível: `crates/cardeal-server/examples/carga.rs`. As telas de negócio no
navegador (migração das telas do desktop para o modo assíncrono) ficaram para uma etapa
posterior — plano em `docs/20-cliente-web.md` §3.

## Consequências

### Positivas

- O domínio não muda uma linha. O servidor é um anfitrião de motores que já existem.
- Custo marginal de uma empresa ociosa = espaço em disco.
- Backup, exportação, restauração e "apagar meus dados" (LGPD) atuam sobre um arquivo.
- Web e desktop são o mesmo programa.

### Negativas

- A primeira requisição de uma empresa fria paga a abertura e a verificação de migrações
  (~5–30 ms).
- Relatório agregado **entre** empresas (painel do SaaS) exige varrer arquivos. É aceitável,
  porque é operação de bastidor.
- Canvas no navegador: sem Ctrl+F nem seleção nativa (ver alternativas).
- A conta (diretório) e o usuário (empresa) são duas entidades. A senha da conta é a que vale
  online. O hash local em `nucleo_usuario` de uma empresa provisionada pelo servidor é aleatório
  e inutilizável.

## Quando revisitar

- Se uma única empresa precisar de mais escrita do que um escritor SQLite entrega (ordem de
  milhares de commits/s sustentados).
- Se o produto passar a exigir consultas transversais entre empresas em tempo real.
- Se medições de uso real mostrarem a aba web acima de 120 MB ou se acessibilidade virar
  requisito contratual. Nesse caso, reavaliar um cliente DOM para as telas de consulta.
