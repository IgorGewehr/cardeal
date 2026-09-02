# ADR-0012 — Um único escritor com group commit

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0003, ADR-0002

## Contexto

SQLite ([ADR-0003](0003-sqlite-como-motor-de-armazenamento.md)) só permite uma transação de escrita
por vez em qualquer configuração — isso não é uma limitação que escolhemos, é uma propriedade do
motor. A questão de arquitetura real é como o sistema se organiza em torno dessa restrição: deixar
vários terminais competirem por uma única conexão de escrita compartilhada (aceitando `SQLITE_BUSY`,
retry, lock ordering) ou desenhar deliberadamente **um único caminho de escrita** por onde toda
mutação do sistema passa, serializada por natureza.

O produto exige durabilidade total: uma operação só é "confirmada" depois que o `fsync` do commit
retornou ([doc 03 §2](../03-pilar-resiliencia.md)), e `synchronous=FULL` é inegociável no escritor
([doc 07 §2](../07-persistencia-sqlite.md)). Um `fsync` por commit individual é caro — 12 ms em um
HDD 5400 rpm típico de PDV mais antigo, 0,3 ms em SSD NVMe ([doc 02 §2.4](../02-pilar-eficiencia.md))
— e se cada operação pagar esse custo isoladamente, o throughput de escrita cai proporcionalmente ao
custo do disco mais lento que o sistema precisa suportar.

Ao mesmo tempo, o volume real de escrita concorrente de uma PME é pequeno: mesmo um comércio com
vários caixas simultâneos gera poucas vendas por minuto no pico, não por segundo. A pergunta que essa
ADR resolve, então, não é "como suportar muitos escritores concorrentes" (o SQLite já responde "não
dá, e não precisa"), mas "como fazer o único escritor possível render o suficiente para nunca ser
percebido como limitação, e ganhar de graça toda a simplificação que vem de nunca ter dois escritores
disputando o mesmo recurso".

## Alternativas consideradas

| Critério | **Escritor único + group commit** | Commit individual por operação | Múltiplas conexões de escrita com retry/`SQLITE_BUSY` | Escrever direto sem fila (cada terminal abre sua própria transação) |
|---|---|---|---|---|
| Vendas/s sustentadas em HDD 5400 rpm | **78,3** | 4,1 | variável, degrada sob contenção | equivalente ao commit individual, mais overhead de retry |
| Vendas/s sustentadas em SSD NVMe | **2.900** | 620 | variável, degrada sob contenção | equivalente ao commit individual |
| Deadlock possível | não (uma única transação de escrita por vez) | não | sim (lock ordering entre conexões) | sim |
| `SQLITE_BUSY` a tratar | não (fila interna, sem contenção externa) | não | sim, exige retry loop em toda escrita | sim, constantemente |
| Modelo mental para a equipe | "escritas são fila, leituras são livres" — uma frase | simples, mas lento | complexo (retry, backoff, lock ordering) | o pior dos dois mundos: lento e complexo |
| Falha de uma operação afeta as demais do lote | não (savepoint por tarefa) | não aplicável (uma por vez) | depende da estratégia de retry | sim, sem isolamento claro |

Múltiplas conexões de escrita com retry — a estratégia mais comum quando times tentam "paralelizar"
SQLite — foi descartada rapidamente: o SQLite já serializa a escrita internamente por trás dessas
conexões, então o ganho de paralelismo é ilusório, e o custo real (retry loops, `SQLITE_BUSY`
tratado em toda operação, risco de lock ordering) é puro. O escritor único não abre mão de nenhum
throughput real que essa alternativa entregaria — apenas remove a complexidade de fingir que existe
paralelismo onde não há.

## Decisão

**Toda escrita do sistema passa por uma única thread com uma única conexão ao banco, que agrupa
operações que chegam em uma janela de 2 ms (até 64 por lote) e resolve o lote inteiro em um único
`fsync`.**

O mecanismo, detalhado no [doc 07 §3 e §4](../07-persistencia-sqlite.md): terminais e tarefas
agendadas enviam suas operações para uma fila (`mpsc`, bounded 512); a thread do escritor bloqueia em
`recv()` (0% de CPU ociosa) até a primeira chegar, então recolhe o que mais chegar em até 2 ms ou até
64 operações, abre `BEGIN IMMEDIATE`, executa cada operação dentro de seu próprio `SAVEPOINT`, e
fecha com um único `COMMIT`. Uma falha de validação em uma operação do lote desfaz só até o
`SAVEPOINT` dela — as outras 63 seguem normalmente. Resultado medido: **4,1 → 78,3 vendas/s em HDD**
e **620 → 2.900 vendas/s em SSD NVMe**, em ambos os casos com durabilidade total
(`synchronous=FULL`).

### O que isso elimina de graça

Porque nunca existem duas transações de escrita simultâneas, o escritor único elimina inteiramente,
por construção e não por disciplina: deadlock, `SQLITE_BUSY`, necessidade de lock ordering entre
conexões, e retry de transação abortada por contenção — toda a classe de anomalia de isolamento entre
escritas concorrentes desaparece porque a serialização total é o isolamento mais forte que existe
([doc 07 §3](../07-persistencia-sqlite.md)).

## Consequências

### Positivas

- Ganho de throughput medido de ~19× em HDD e ~4,7× em SSD sobre commit individual, sem abrir mão de
  nenhuma durabilidade — o `fsync` de cada lote ainda é síncrono e total.
- Deadlock, `SQLITE_BUSY` e lock ordering deixam de existir como categoria de bug a testar ou a
  corrigir — não há código de retry a escrever, testar e manter em nenhum módulo.
- O modelo mental para quem entra na equipe cabe em uma frase — "escritas são uma fila; leituras são
  livres" ([doc 07 §3](../07-persistencia-sqlite.md)) — o que acelera diretamente o onboarding
  descrito no [doc 16](../16-onboarding.md).
- O teto de throughput resultante (78,3 vendas/s no pior hardware medido) está muito acima do pico
  real de qualquer PME — mesmo um comércio movimentado gera da ordem de 2 vendas por minuto no pico,
  não por segundo ([doc 16](../16-onboarding.md)).
- A `UnidadeDeTrabalho` acumula os eventos de domínio publicados e os grava no outbox na mesma
  transação do fato ([doc 07 §3](../07-persistencia-sqlite.md)) — nenhum evento se perde nem é
  publicado sem o fato ter realmente acontecido, de graça, porque tudo passa pelo mesmo commit.

### Negativas

- Existe um teto físico de throughput de escrita do sistema inteiro — mesmo sendo duas ordens de
  grandeza acima da necessidade de uma PME, ele é real e finito, e não escala horizontalmente
  adicionando mais hardware.
- O escritor é um ponto único de latência: qualquer degradação nele (disco lento, contenção de I/O
  do sistema operacional) afeta toda escrita do sistema simultaneamente, sem isolamento entre
  terminais.
- Uma operação anormalmente lenta dentro de um lote (por exemplo, uma validação de negócio cara)
  atrasa a resposta de todas as outras operações do mesmo lote, porque o `COMMIT` só acontece depois
  que todas as tarefas do lote terminam de executar.
- A arquitetura inteira depende dessa thread única estar sempre saudável — um bug que trave a thread
  do escritor (por exemplo, um laço infinito dentro de uma validação) para toda a escrita do sistema
  de uma vez, sem um caminho alternativo.

### Mitigações

- Cada tarefa roda em seu próprio `SAVEPOINT`, então uma tarefa lenta ou com erro de validação não
  derruba as demais do lote — o isolamento de falha é por tarefa, não por lote inteiro
  ([doc 07 §4](../07-persistencia-sqlite.md)).
- Toda tarefa tem timeout — uma operação que ultrapassa o limite é abortada e devolve erro de domínio
  ao chamador, em vez de travar o lote indefinidamente.
- A profundidade da fila de escrita é uma métrica monitorada continuamente: um crescimento sustentado
  sinaliza contenção antes que o usuário sinta atraso perceptível, dando margem para investigar antes
  de virar incidente.
- O teto de throughput medido está tão acima da necessidade real de uma PME (78,3 vendas/s contra um
  pico real da ordem de dezenas por minuto) que a mitigação mais eficaz é, na prática, a folga
  arquitetural embutida na escolha — o sistema teria de crescer duas ordens de grandeza em volume de
  negócio para essa restrição começar a apertar.

## Quando revisitar

- Se a profundidade média da fila de escrita, medida em produção, ultrapassar sistematicamente
  100 ms de espera em horário de pico de qualquer cliente real — hoje isso exigiria uma carga muito
  acima do cenário sintético de referência usado para calibrar o orçamento (`supermercado_bairro`, 3
  PDVs, 1.200 cupons/dia, [doc 02 §4](../02-pilar-eficiencia.md)).
- Se o roadmap de multi-loja com consolidação central ([doc 17 — Fase 6](../17-roadmap.md)) exigir um
  volume de escrita agregado (múltiplas filiais escrevendo no mesmo servidor central) que se aproxime
  do teto medido — nesse ponto, a arquitetura de escritor único por servidor continuaria válida, mas
  a topologia (um servidor por filial, não um único servidor central) precisaria ser reforçada como
  obrigatória, não apenas recomendada.
- Se um cliente de volume atípico (por exemplo, um posto de grande porte com dezenas de bicos e alta
  frequência de transação) se aproximar do teto de throughput medido em hardware real de produção —
  hoje não há evidência disso, mas seria o gatilho concreto para investigar particionamento por
  domínio (por exemplo, um escritor por módulo de alto volume) em vez de escritor único global.
- Não revisitaríamos essa decisão pela existência do teto em si — todo sistema tem um teto de
  throughput; o teste relevante é se ele algum dia se aproxima do que uma PME real gera, e hoje a
  margem é de duas ordens de grandeza.
