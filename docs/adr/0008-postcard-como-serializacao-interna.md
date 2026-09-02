# ADR-0008 — postcard como formato binário interno

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0009, ADR-0003

## Contexto

O mesmo dado serializado circula por caminhos muito diferentes dentro do Cardeal: o corpo de um
comando enviado do cliente ao servidor pela rede, a carga de um evento gravado no outbox transacional
dentro da mesma transação SQLite ([doc 06 §2](../06-modelo-de-dados.md)), o diário local do carrinho
no terminal (append-only, `fsync` a cada mutação, [doc 03 §2.1](../03-pilar-resiliencia.md)), e o
cache local do posto avançado em modo autônomo ([doc 03 §3.1](../03-pilar-resiliencia.md)). Em todos
esses lugares, o formato precisa ser compacto (o diário do carrinho é escrito a cada item bipado,
muitas vezes por segundo em pico de PDV), rápido de serializar e desserializar (está no caminho quente
de toda escrita) e determinístico o bastante para comparação em testes de ouro.

Ao mesmo tempo, o protocolo entre cliente e servidor ([doc 09](../09-protocolo-api.md)) precisa
oferecer JSON para integração externa e depuração — nem todo consumidor do sistema fala Rust, e um
desenvolvedor investigando um problema em produção precisa conseguir olhar o payload sem uma
ferramenta especial. Isso significa que o formato binário interno não precisa ser autodescritivo (o
contrato já é imposto pelo tipo Rust compartilhado em `cardeal-protocol`), mas o sistema como um todo
precisa manter um caminho de depuração acessível por fora dele.

O crate `cardeal-protocol` já garante que servidor e cliente compilam o mesmo contrato tipado
([doc 09 §1](../09-protocolo-api.md)) — uma incompatibilidade de esquema já é pega em compilação, não
em runtime. Isso muda o cálculo de trade-off de formato: não precisamos de um formato autodescritivo
para produção porque o tipo já garante o contrato; precisamos de um formato rápido e compacto que
sirva a todos os caminhos internos com a mesma implementação.

## Alternativas consideradas

| Critério | **postcard** | JSON | MessagePack | bincode | Protobuf | CBOR | FlatBuffers |
|---|---|---|---|---|---|---|---|
| Tamanho típico (comparado a JSON = 100%) | ~35–45% | 100% | ~55–65% | ~40–50% | ~40–50% | ~60–70% | maior (padding de alinhamento) |
| Velocidade de serialização | muito alta | baixa-média | média | muito alta | média (requer geração de código) | média | muito alta (zero-copy nativo) |
| Auto-descritivo (legível sem esquema) | não | sim | parcial | não | não (requer `.proto`) | parcial | não |
| `no_std` / funciona sem alocador | sim | não (implementações típicas alocam) | não nativamente | parcial | não | parcial | sim |
| Evolução de esquema (campo novo, campo removido) | manual, via `#[non_exhaustive]` e cuidado do time | trivial (campo ignorado) | trivial | manual, frágil a mudança de ordem | forte (nativo ao formato) | trivial | forte (nativo ao formato) |
| Ferramentas de depuração prontas (inspecionar payload) | nenhuma nativa | qualquer editor de texto, `curl` | ferramentas de terceiro | nenhuma nativa | `protoc` + plugins | algumas | ferramentas próprias, verbosas |
| Ecossistema fora de Rust | praticamente nenhum | universal | amplo (múltiplas linguagens) | praticamente nenhum (é específico de Rust) | amplo, padrão de indústria | amplo | amplo, mas pesado para adotar |
| Maturidade e uso em produção fora deste projeto | crescente, usado em embarcados Rust | universal | maduro | maduro no ecossistema Rust | maduro, padrão de indústria | maduro | maduro, mas mais usado em jogos/grandes payloads |

Protobuf foi a alternativa mais seriamente considerada por ser o padrão de indústria com melhor
suporte a evolução de esquema. Foi descartado porque exige um compilador de esquema externo
(`protoc`) e geração de código fora do fluxo natural do `cargo`, além de ser mais verboso e mais lento
para os payloads pequenos e frequentes do caminho quente do Cardeal (uma partida de lançamento, um
item de carrinho) — onde `postcard`, aproveitando `serde` diretamente sobre os mesmos tipos Rust que
já definem o domínio, tem menos fricção de manutenção.

## Decisão

**postcard é o formato binário padrão para todo o transporte interno — comandos e eventos na rede,
outbox transacional, diário local durável e cache do posto avançado — com JSON disponível por
negociação de `Content-Type` para integração externa e depuração.**

A negociação está descrita no [doc 09 §4](../09-protocolo-api.md): `application/x-cardeal-postcard` é
o padrão entre componentes do próprio Cardeal; JSON é servido quando o cliente pede
(`?formato=json` em desenvolvimento, ou por `Content-Type` em produção para integrações de terceiros
via [doc 09 §8](../09-protocolo-api.md)). Como `postcard` é compacto (sem nomes de campo no payload,
apenas os valores na ordem do `struct`), zero-copy na desserialização e implementado em `no_std`, o
mesmo formato e a mesma implementação servem igualmente ao log durável em disco, ao outbox e à rede —
uma única biblioteca, um único caminho de código de serialização para auditar e otimizar.

## Consequências

### Positivas

- Payloads significativamente menores que JSON (tipicamente 35–45% do tamanho) em todo o caminho
  quente — rede, outbox, diário local — o que ajuda diretamente o orçamento de I/O e de energia do
  [doc 02](../02-pilar-eficiencia.md).
- Serialização e desserialização muito rápidas, sem alocação de árvore intermediária, coerente com a
  regra de não alocar em caminho quente ([doc 02 §2.6](../02-pilar-eficiencia.md)).
- `no_std` significa que a mesma implementação serve tanto o servidor quanto código de baixo nível
  (drivers, o diário local do carrinho) sem depender de um runtime completo.
- Um único formato e uma única biblioteca cobrindo rede, outbox e log durável reduz a superfície de
  bugs de serialização a um lugar — não há um formato para "gravar em disco" e outro para "enviar pela
  rede" que possam divergir sutilmente.
- JSON continua disponível por negociação de conteúdo, preservando um caminho simples de depuração e
  de integração para quem não fala `postcard` nem Rust ([doc 09 §4](../09-protocolo-api.md)).

### Negativas

- `postcard` não é autodescritivo: um payload binário sozinho, sem o tipo Rust correspondente, não diz
  nada sobre sua própria estrutura — depurar um payload capturado exige o código-fonte do tipo, não
  apenas o dado.
- Praticamente nenhum ecossistema fora de Rust: uma integração de terceiro que precise falar
  diretamente `postcard` (em vez de usar a via JSON) não tem bibliotecas prontas em Python, Java,
  JavaScript etc.
- Evolução de esquema é uma responsabilidade manual da equipe, não uma garantia do formato: adicionar
  um campo exige disciplina (`Option<T>` com `#[serde(default)]`, ordem estável de campos) — um erro
  aqui quebra compatibilidade silenciosamente, sem o formato reclamar.
- Sem ferramentas prontas de terceiro para inspecionar um payload capturado (como `protoc --decode`
  faz para Protobuf) — a equipe depende de instrumentação própria para depuração fora do fluxo normal
  de log.

### Mitigações

- JSON permanece disponível em todo o protocolo por negociação de `Content-Type`, cobrindo tanto
  depuração em desenvolvimento (`?formato=json`) quanto integração externa de produção
  ([doc 09 §4](../09-protocolo-api.md), [doc 09 §8](../09-protocolo-api.md)) — ninguém depende
  exclusivamente do binário para entender o sistema.
- Todos os enums públicos do protocolo são `#[non_exhaustive]` ([doc 09 §7](../09-protocolo-api.md)):
  um cliente mais antigo que recebe uma variante desconhecida faz fallback controlado em vez de falhar
  — a evolução de esquema é uma disciplina verificada em compilação, não só em convenção.
- A suíte de testes guarda envelopes serializados de versões anteriores em `testes/ouro/protocolo/` e
  valida continuamente que ainda desserializam ([doc 09 §7](../09-protocolo-api.md)) — regressão de
  compatibilidade é pega em CI, não em produção.
- `protocolo: u16` no envelope, com o servidor aceitando uma janela de versões (`[atual-2, atual]`),
  dá margem de manobra para clientes desatualizados sem quebrar de imediato
  ([doc 09 §7](../09-protocolo-api.md)).

## Quando revisitar

- Se uma integração de terceiro relevante exigir consumir `postcard` diretamente (sem passar pela via
  JSON) e a ausência de bibliotecas fora de Rust se tornar um bloqueador de negócio real, não apenas
  uma preferência.
- Se um bug de compatibilidade de esquema chegar a produção apesar dos testes de ouro — sinal de que a
  disciplina manual de evolução de esquema não está sendo suficiente e o formato deveria oferecer mais
  garantia estrutural (por exemplo, migrar para Protobuf ou CBOR nesse ponto específico).
- Se o custo de manter instrumentação própria de depuração de payload binário (em vez de usar
  ferramentas prontas de mercado) superar consistentemente o ganho de desempenho e tamanho que o
  formato entrega.
- Se o projeto vier a precisar de um caminho de dados que atravesse, nativamente, outro runtime não
  Rust (por exemplo, um componente WASM de terceiro) e passar por JSON nesse ponto tiver custo de
  desempenho inaceitável.
