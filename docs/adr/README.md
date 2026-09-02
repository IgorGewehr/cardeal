# ADRs — Registro de decisões de arquitetura

> Um ADR (*Architecture Decision Record*) documenta uma decisão de arquitetura que tem peso
> suficiente para valer a pena registrar o contexto, as alternativas descartadas e os motivos —
> não só a decisão em si. Se você discorda de algo aqui, o primeiro passo não é mudar o código: é
> abrir uma discussão referenciando o ADR e, se a decisão mudar, criar um novo ADR que o substitua
> (marcando o antigo como **Substituída**), nunca editar silenciosamente um ADR aceito.

Todos os ADRs seguem o mesmo formato: Contexto, Alternativas consideradas, Decisão, Consequências
(positivas, negativas, mitigações) e Quando revisitar. As seções "Negativas" e "Quando revisitar" são
deliberadamente honestas — um ADR sem trade-off real não está fazendo seu trabalho.

## Índice

| # | Título | Status | Data |
|---|---|---|---|
| [0001](0001-rust-como-linguagem-unica.md) | Rust como linguagem única do sistema | Aceita | 2026-09-01 |
| [0002](0002-partidas-dobradas-como-nucleo.md) | Partidas dobradas como núcleo de integração | Aceita | 2026-09-01 |
| [0003](0003-sqlite-como-motor-de-armazenamento.md) | SQLite embarcado como armazenamento padrão | Aceita | 2026-09-01 |
| [0004](0004-egui-como-camada-de-interface.md) | egui/eframe como camada de interface | Aceita | 2026-09-01 |
| [0005](0005-event-sourcing-restrito-ao-razao.md) | Event sourcing restrito ao razão, CRUD auditado no resto | Aceita | 2026-09-01 |
| [0006](0006-fiscal-via-api-externa.md) | Delegar emissão fiscal a uma API externa | Aceita | 2026-09-01 |
| [0007](0007-duckdb-e-parquet-para-analitico.md) | DuckDB + Parquet para a carga analítica | Aceita | 2026-09-01 |
| [0008](0008-postcard-como-serializacao-interna.md) | postcard como formato binário interno | Aceita | 2026-09-01 |
| [0009](0009-transporte-in-process.md) | Transporte em memória no modo monoposto | Aceita | 2026-09-01 |
| [0010](0010-idioma-do-codigo.md) | Domínio em português, infraestrutura em inglês | Aceita | 2026-09-01 |
| [0011](0011-arquivamento-de-exercicios.md) | Arquivar exercícios encerrados em bases anexadas somente leitura | Aceita | 2026-09-01 |
| [0012](0012-escritor-unico-com-group-commit.md) | Um único escritor com group commit | Aceita | 2026-09-01 |
| [0013](0013-uuidv7-como-identidade.md) | UUIDv7 como chave primária, número sequencial como identificador do usuário | Aceita | 2026-09-01 |
| [0014](0014-modularidade-em-runtime-por-manifesto.md) | Modularidade resolvida em runtime por manifesto declarativo | Aceita | 2026-09-01 |

## Como usar

- Antes de propor uma mudança que contrarie um ADR aceito, leia o ADR inteiro — em especial "Quando
  revisitar". Se nenhum gatilho listado se aplica ao seu caso, o ônus da prova de que a decisão deve
  mudar é seu, não do ADR.
- Um ADR novo referencia os ADRs relacionados que já existem. Não duplique contexto já registrado —
  linke.
- Numeração é sequencial e nunca reaproveitada, mesmo que um ADR seja substituído.
