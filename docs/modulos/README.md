# Módulos do Cardeal

Especificação funcional e técnica de cada módulo de negócio. Cada arquivo segue a mesma estrutura
de 13 seções (escopo, submódulos, entidades, máquinas de estado, comandos, consultas, receituário
contábil, eventos, permissões, telas, regras de negócio críticas, comportamento offline, tabelas),
descrita em [`doc 04 §7`](../04-pilar-modularidade.md#7-criando-um-módulo-novo-visão-geral) e
[`doc 15 §2`](../15-convencoes-codigo.md#2-estrutura-de-um-módulo).

Leia primeiro [`doc 05 — Núcleo financeiro`](../05-nucleo-financeiro.md): o Razão é o barramento de
integração e a seção 7 de cada módulo abaixo (**Receituário contábil**) é uma extensão direta da
tabela do doc 05 §5. Nenhum módulo lança dinheiro fora desse contrato.

## Índice

| Módulo | Fase do roadmap | Depende de | Perfis que usam |
|---|---|---|---|
| [`financeiro`](financeiro.md) | Fase 1 — Financeiro + Clientes | núcleo | Todos |
| [`clientes`](clientes.md) | Fase 1 — Financeiro + Clientes | núcleo | Todos |
| [`estoque`](estoque.md) | Fase 2 — Estoque + Vendas + PDV | núcleo | Comércio, Posto, Micro indústria, Hotel/pousada, Assistência técnica |
| [`vendas`](vendas.md) | Fase 2 — Estoque + Vendas + PDV | financeiro, clientes, estoque | MEI/autônomo, Comércio, Posto, Micro indústria, Assistência técnica, Serviços |
| [`pdv`](pdv.md) | Fase 2 — Estoque + Vendas + PDV | vendas | Comércio, Posto |
| [`compras`](compras.md) | Fase 3 — Compras + Fiscal completo | estoque, financeiro | Comércio, Posto, Micro indústria, Assistência técnica |
| [`crm`](crm.md) | Fase 5 — Verticais | clientes | Serviços/consultoria, Comércio (opcional) |
| [`agenda`](agenda.md) | Fase 5 — Verticais | núcleo | Assistência técnica, Serviços/consultoria, Hotel/pousada |
| [`os`](os.md) | Fase 5 — Verticais | vendas, agenda | Assistência técnica |
| [`orcamentos`](orcamentos.md) | Fase 5 — Verticais | núcleo (melhora com `os`) | Assistência técnica, Serviços/consultoria, Micro indústria |
| [`alugueis`](alugueis.md) | Fase 5 — Verticais | financeiro, clientes | Locadora |
| [`combustivel`](combustivel.md) | Fase 5 — Verticais | estoque, pdv | Posto de combustível |
| [`hotelaria`](hotelaria.md) | Fase 5 — Verticais | financeiro, agenda | Hotel/pousada |
| [`industria`](industria.md) | Fase 5 — Verticais | estoque | Micro indústria |
| [`contabil`](contabil.md) | Fase 5 — Verticais | financeiro | Todos (opcional, sob demanda do contador) |

## Como ler

- **Escopo** delimita o módulo por negativa tanto quanto por positiva — "não faz" evita que dois
  módulos disputem a mesma responsabilidade.
- **Receituário contábil** (seção 7) é obrigatório e testado por `testes/receituario.rs` em cada
  crate de módulo (`testkit::assert_lancamento!`), conforme [`doc 05 §5`](../05-nucleo-financeiro.md#5-o-receituário-como-cada-módulo-posta).
- **Tabelas** (seção 13) seguem as convenções do [`doc 06`](../06-modelo-de-dados.md): `STRICT`,
  `id BLOB` (UUIDv7), dinheiro em `INTEGER` (centavos), data em `INTEGER` (dias desde 1970-01-01),
  instante em `INTEGER` (microssegundos UTC), enum em `TEXT` com `CHECK`, coluna `versao` em toda
  entidade mutável, coluna `empresa` como primeira coluna de todo índice de negócio.
- O núcleo transacional (`financeiro`, `clientes`, `estoque`, `vendas`, `pdv`, `compras`, `agenda`,
  `os`, `orcamentos`) pode depender diretamente uns dos outros no `Cargo.toml` — evoluem juntos e
  estão presentes na maioria dos perfis. Já os módulos verticais de segmento (`crm`, `alugueis`,
  `combustivel`, `hotelaria`, `industria`, `contabil`) **nunca** dependem de outro módulo no
  `Cargo.toml` — nem entre si, nem do núcleo — para continuarem ligáveis/desligáveis por perfil sem
  arrastar código de um segmento alheio ([`doc 04 §8`](../04-pilar-modularidade.md#8-regras-invioláveis-de-módulo)).
  Nos dois casos, quando não houver dependência direta, a comunicação acontece pelo Razão, por
  eventos de domínio (`modulo.substantivo_particípio.vN`) ou por portas (traits) resolvidas na
  composição do `cardeal-server`.
