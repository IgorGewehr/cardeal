# ADR-0013 — UUIDv7 como chave primária, número sequencial como identificador do usuário

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0003, ADR-0002

## Contexto

O modo autônomo é um requisito central do produto: um terminal de PDV precisa continuar vendendo
mesmo sem servidor acessível, gerando registros localmente que serão reconciliados depois
([doc 03 §3](../03-pilar-resiliencia.md)). Isso significa que o identificador primário de qualquer
entidade — um lançamento, uma venda, um item de estoque — precisa poder ser gerado **no terminal, sem
falar com o servidor**, e ainda assim ser globalmente único quando a reconciliação acontecer. Um
`INTEGER AUTOINCREMENT` tradicional não serve: dois terminais offline gerariam o mesmo número, e
resolver esse conflito na reconciliação exigiria uma etapa de renumeração que, por sua vez, quebraria
qualquer referência já impressa em um cupom ou já mostrada ao usuário.

Ao mesmo tempo, a chave primária de toda tabela de negócio ([doc 06 §1](../06-modelo-de-dados.md)) é a
coluna mais consultada e mais indexada do sistema — ela aparece em toda junção, todo índice
estrangeiro, toda ordenação temporal implícita de "o que aconteceu depois do quê". A escolha de
formato de identificador tem impacto direto e mensurável em como a árvore B-tree do SQLite se
comporta sob inserção contínua: um identificador aleatório (UUIDv4 clássico) insere em posições
aleatórias da árvore, fragmentando páginas e degradando localidade; um identificador ordenado no
tempo insere sempre à direita, mantendo a árvore compacta.

Por fim, existe uma tensão de usabilidade: ninguém no balcão vai ler ou digitar um UUID de 36
caracteres em hexadecimal — o número que o operador vê no cupom, o número que o cliente cita ao
telefone, precisa ser curto e sequencial dentro do contexto da empresa (`Venda nº 4218`, não
`Venda nº 019234a1-...`).

## Alternativas consideradas

| Critério | **UUIDv7 + número sequencial separado** | `INTEGER AUTOINCREMENT` | UUIDv4 | ULID | Snowflake (id de 64 bits com timestamp + worker id) |
|---|---|---|---|---|---|
| Gerável offline, sem coordenação com servidor | sim | **não** — exige o servidor para garantir unicidade | sim | sim | sim, mas exige "worker id" único por terminal pré-atribuído |
| Ordenado no tempo (localidade de B-tree) | sim | sim (mas não sobrevive a geração offline) | não — aleatório, fragmenta o índice | sim | sim |
| Legível/citável por humano | não (mas há coluna `numero` dedicada para isso) | sim, nativamente | não | parcialmente (base32, ainda longo) | não |
| Tamanho de armazenamento | 16 bytes | 8 bytes (ou menos) | 16 bytes | 16 bytes (128 bits) | 8 bytes |
| Padrão amplamente suportado (RFC) | sim (RFC 9562) | trivial (nativo de qualquer SGBD) | sim (RFC 9562, variante v4) | não é RFC, é convenção de comunidade | não é padrão, é padrão de fato do Twitter/X |
| Risco de colisão entre terminais offline | desprezível (aleatoriedade + timestamp de 48 bits) | **inaceitável sem servidor** | desprezível | desprezível | depende de gestão correta de worker id — risco humano de reuso |

`INTEGER AUTOINCREMENT` foi descartado de saída pelo requisito de geração offline — é a alternativa
mais simples e mais legível, mas simplesmente não resolve o problema central. Snowflake foi
descartado por exigir gestão de "worker id" único por terminal, uma peça de configuração a mais que
pode falhar silenciosamente (dois terminais reusando o mesmo worker id por erro de provisionamento)
— UUIDv7 não tem essa classe de risco porque não depende de nenhuma coordenação de identidade de
terminal.

## Decisão

**Toda entidade de negócio usa UUIDv7 como chave primária (`BLOB(16)`), gerado localmente e nunca
pelo servidor; o número legível para o usuário (`numero INTEGER`) é uma coluna separada, sequencial
por empresa, atribuída pelo servidor no momento do commit.**

A convenção está fixada no [doc 06 §1](../06-modelo-de-dados.md): PK sempre `id BLOB(16)` UUIDv7,
ordenável no tempo, sem hotspot de índice. UUIDv7 embute um timestamp de 48 bits nos bits mais
significativos, o que faz cada novo identificador inserir sempre à direita da árvore B-tree — o mesmo
comportamento de um `AUTOINCREMENT`, mas sem exigir coordenação central. O `numero` sequencial (por
exemplo, o número da nota, do lançamento no razão — [doc 05 §3.2](../05-nucleo-financeiro.md)) é
atribuído separadamente pelo servidor no commit, exatamente para servir ao propósito humano que o
UUID não serve: aparecer no cupom, ser citado ao telefone, ser digitado numa busca. Tabelas com chave
primária composta e linhas pequenas (`razao_partida`, tabelas de junção) usam `WITHOUT ROWID`, que
elimina o índice secundário implícito do SQLite e economiza cerca de 18% de espaço na maior tabela do
sistema ([doc 06 §3](../06-modelo-de-dados.md)).

## Consequências

### Positivas

- Qualquer terminal, mesmo totalmente offline, gera identificadores globalmente únicos sem falar com
  ninguém — pré-condição direta do modo autônomo ([doc 03 §3](../03-pilar-resiliencia.md)).
- A ordenação temporal embutida no UUIDv7 mantém a árvore B-tree compacta sob inserção contínua, sem o
  custo de fragmentação que um UUIDv4 aleatório causaria na mesma carga.
- O usuário nunca vê um UUID: o `numero` sequencial por empresa é o que aparece em toda interface,
  cupom e busca — a complexidade técnica do identificador é inteiramente invisível à operação.
- UUIDv7 é um padrão formalizado (RFC 9562), não uma convenção proprietária do projeto — ferramentas e
  bibliotecas de terceiro que já suportam UUID funcionam sem adaptação.
- `WITHOUT ROWID` em tabelas de chave composta e linha pequena (como `razao_partida`, a maior tabela
  do sistema) economiza espaço e uma indireção de busca por leitura, efeito colateral positivo direto
  da escolha de modelagem de identidade.

### Negativas

- 16 bytes por identificador contra 8 bytes (ou menos) de um `INTEGER AUTOINCREMENT` — em uma tabela
  com dezenas de milhões de linhas e várias colunas de chave estrangeira, isso se traduz em espaço em
  disco real adicional (a diferença de 8 bytes por referência de chave estrangeira, multiplicada por
  ~13 milhões de partidas e múltiplas colunas de referência, chega à ordem de dezenas de megabytes).
- Ilegível para humanos: ninguém identifica ou memoriza um UUID, e digitar um manualmente (por
  exemplo, ao relatar um bug) é impraticável — daí a necessidade da coluna `numero` paralela.
- UUIDv7 vaza informação de timestamp de criação embutida no próprio identificador — em um contexto
  onde o identificador é exposto externamente (por exemplo, em uma URL de API pública), isso revela
  quando o registro foi criado, o que pode não ser desejável em todo cenário.
- Comparação e indexação de 16 bytes é marginalmente mais cara em CPU que 8 bytes, ainda que
  desprezível na escala de operações por segundo que o sistema sustenta.

### Mitigações

- A coluna `numero` sequencial por empresa, atribuída no momento do commit pelo servidor
  ([doc 06 §1](../06-modelo-de-dados.md)), resolve inteiramente o problema de legibilidade — o usuário
  nunca interage com o UUID diretamente, apenas com o número.
- `WITHOUT ROWID` é aplicado deliberadamente onde compensa (chave composta, linha pequena, como
  `razao_partida` e tabelas de junção), reduzindo o overhead de espaço que a escolha de UUID de 16
  bytes introduziria de outra forma ([doc 06 §3](../06-modelo-de-dados.md)).
- O vazamento de timestamp embutido no UUIDv7 é aceito conscientemente para identificadores internos
  (nunca expostos como segredo de autenticação) — nenhum UUID de entidade de negócio é usado como
  token de sessão ou como segredo, papel que cabe a estruturas dedicadas do `cardeal-auth`
  ([doc 08 §2](../08-seguranca-permissoes.md)).
- O custo adicional de espaço em disco é pequeno frente ao tamanho total esperado da base
  (~2,4 GB para três anos de um comércio médio, [doc 06 §6](../06-modelo-de-dados.md)) e é, de
  qualquer forma, dominado pelo plano de arquivamento de exercícios
  ([ADR-0011](0011-arquivamento-de-exercicios.md)) muito antes de se tornar um problema real.

## Quando revisitar

- Se uma API pública de integração ([doc 09 §8](../09-protocolo-api.md)) vier a expor identificadores
  de entidade diretamente a terceiros de forma que o vazamento de timestamp embutido no UUIDv7 se
  torne uma preocupação real de privacidade ou de segurança competitiva (por exemplo, revelar volume
  de operação por padrão temporal de IDs).
- Se o overhead de espaço em disco de 16 bytes por identificador, multiplicado pela escala real de
  uma instalação grande, se tornar mensuravelmente relevante frente ao orçamento de armazenamento —
  hoje isso não se materializa antes do gatilho de arquivamento de 20 GB
  ([ADR-0011](0011-arquivamento-de-exercicios.md)).
- Se o modo autônomo evoluir para permitir coordenação parcial entre terminais offline (por exemplo,
  sincronização par a par sem servidor central) de um jeito que abra uma nova classe de requisito de
  identidade não coberta pela geração local independente que o UUIDv7 hoje resolve.
- Não revisitaríamos por "UUID é feio" ou "eu preferia inteiros" isoladamente — o requisito de geração
  offline sem coordenação é inegociável enquanto o modo autônomo for um pilar do produto, e nenhuma
  alternativa investigada resolve esse requisito sem reintroduzir dependência de coordenação central.
