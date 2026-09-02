# Documentação do Cardeal

Ordem de leitura recomendada para quem está chegando: **16 → 00 → 01 → 05 → 04**.
Depois, o resto sob demanda.

## Fundamentos

| # | Documento | Do que trata |
|---|---|---|
| 00 | [Visão de produto](00-visao-produto.md) | Para quem é, quais perfis de empresa atende, o que explicitamente não fazemos |
| 01 | [Arquitetura geral](01-arquitetura-geral.md) | Camadas, crates, o caminho completo de um comando |

## Os três pilares

| # | Documento | Do que trata |
|---|---|---|
| 02 | [Pilar I — Eficiência](02-pilar-eficiencia.md) | Orçamento de RAM/CPU/energia, técnicas, como medimos |
| 03 | [Pilar II — Resiliência](03-pilar-resiliencia.md) | Queda de energia, queda de rede, modo autônomo, sincronização |
| 04 | [Pilar III — Modularidade](04-pilar-modularidade.md) | Manifesto de módulo, perfis de empresa, ativação em runtime |

## Núcleo

| # | Documento | Do que trata |
|---|---|---|
| 05 | [Núcleo financeiro](05-nucleo-financeiro.md) | Razão de partidas dobradas, previsto × realizado, projeção de fluxo de caixa |
| 06 | [Modelo de dados](06-modelo-de-dados.md) | Convenções, tabelas do núcleo, identidade, versionamento, migrações |
| 07 | [Persistência](07-persistencia-sqlite.md) | SQLite, escritor único, group commit, durabilidade, backup |
| 08 | [Segurança e permissões](08-seguranca-permissoes.md) | Usuários, RBAC/ABAC, sessões, dispositivos, TLS interno, auditoria |
| 09 | [Protocolo e API](09-protocolo-api.md) | Comandos, consultas, transporte, tempo real, idempotência |

## Periferia

| # | Documento | Do que trata |
|---|---|---|
| 10 | [Módulo fiscal](10-modulo-fiscal.md) | Porta fiscal via API, máquina de estados do documento, contingência, SPED |
| 11 | [Analytics e BI](11-analytics-bi.md) | Outbox → Parquet → DuckDB, esquema estrela, base para ML |
| 12 | [UI/UX](12-ui-ux.md) | Design system Rubro, sidebar retrátil, "O Pulso" (financeiro como tela inicial) |
| 13 | [Observabilidade](13-observabilidade.md) | Tracing, métricas, tela de diagnóstico, suporte remoto |

## Engenharia

| # | Documento | Do que trata |
|---|---|---|
| 14 | [Testes e qualidade](14-testes-qualidade.md) | Pirâmide de testes, testes de propriedade, teste de queda de energia, CI |
| 15 | [Convenções de código](15-convencoes-codigo.md) | Idioma, nomes, erros, estrutura de arquivos, revisão |
| 16 | [Onboarding](16-onboarding.md) | O primeiro dia. Como criar seu primeiro módulo em 1 hora |
| 17 | [Roadmap](17-roadmap.md) | Fases, marcos, escopo de cada release |
| 18 | [Glossário](18-glossario.md) | Vocabulário do domínio (contábil, fiscal, varejo) |

## Decisões de arquitetura (ADR)

Registros curtos e imutáveis de **por que** cada escolha estrutural foi feita.
Índice em [`adr/README.md`](adr/README.md).

## Módulos

Especificação funcional e técnica de cada módulo em [`modulos/`](modulos/).
