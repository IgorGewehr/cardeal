# ADR-0003 — SQLite embarcado como armazenamento padrão

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0011, ADR-0012, ADR-0013

## Contexto

O Cardeal atende de um MEI rodando um único processo no próprio notebook até um comércio com 1 a 30
terminais em rede local ([doc 01 §6](../01-arquitetura-geral.md)). Em nenhum desses cenários existe
um administrador de banco de dados no cliente, e a instalação precisa acontecer sem serviço externo,
sem usuário e senha de banco a configurar, sem porta a abrir no firewall. O alvo de produto é claro:
menos de 15 minutos entre o download e a primeira venda ([doc 00 §6](../00-visao-produto.md)).

Ao mesmo tempo, o sistema precisa da durabilidade forte descrita no [doc 03](../03-pilar-resiliencia.md):
uma operação só é "confirmada" depois que o `fsync` do commit retornou, e o teste de queda de energia
(500 ciclos de `SIGKILL` em CI) não pode nunca perder uma venda confirmada nem deixar uma venda pela
metade. O motor de armazenamento escolhido precisa entregar essa garantia sem exigir tuning
especializado do cliente final.

O volume de dados de uma PME brasileira típica é conhecido e mensurável: um comércio médio gera cerca
de 1.200 cupons/dia, e em três anos acumula por volta de 2,4 GB de base
([doc 06 §6](../06-modelo-de-dados.md)). Isso está muito abaixo do território onde bancos
cliente-servidor se justificam pela escala — mas exatamente no território onde a complexidade
operacional de manter um servidor de banco separado é puro custo sem benefício correspondente.

A concorrência de escrita real também é pequena: mesmo em um comércio com vários caixas, o número de
escritas por segundo no pico é baixo (poucas vendas por minuto por terminal). O gargalo teórico de "um
banco embarcado só aceita um escritor por vez" — o argumento clássico contra SQLite em produção —
precisava ser medido, não assumido, antes de descartar a opção mais simples de operar.

## Alternativas consideradas

| Critério | **SQLite** | PostgreSQL | Firebird | SQL Server Express | libSQL/Turso |
|---|---|---|---|---|---|
| RAM em repouso | ~2 MB | ~120 MB (mínimo realista) | ~40 MB | ~200 MB | ~15 MB |
| Instalação pelo cliente | nenhuma (biblioteca embutida) | serviço, usuário, senha, tuning | serviço | serviço, licença, tuning | biblioteca embutida |
| Backup | copiar 1 arquivo | `pg_dump` + agendamento | `gbak` | `bak` + agente | copiar 1 arquivo (ou réplica remota) |
| Durabilidade em queda de energia | excelente (WAL + fsync) | excelente | boa | boa | excelente (herda SQLite) |
| Escritores concorrentes nativos | 1 | muitos | muitos | muitos (limite de 10 GB no Express) | 1 (mesmo modelo do SQLite) |
| Adequado a 1–30 terminais | **sim** | exagero operacional | sim | sim, mas com teto de 10 GB | sim, mas ecossistema jovem |
| Cliente troca o PC do servidor | copia o arquivo | reinstala e restaura dump | reinstala e restaura | reinstala e restaura | copia o arquivo |
| Precedente no ERP brasileiro | incomum em produtos comerciais | comum em ERPs de médio porte | **dominante em ERP BR legado** | comum em soluções Microsoft | nenhum |
| Réplica remota nativa (leitura) | não (requer solução externa) | sim | sim | sim | sim (é o diferencial do Turso) |
| Maturidade e estabilidade do formato de arquivo | 20+ anos, extremamente estável | N/A (protocolo, não arquivo) | estável | estável | recente (fork ativo do SQLite) |

Firebird é a comparação mais relevante para o mercado brasileiro: é o motor de facto atrás de boa
parte dos ERPs legados do setor. Descartamos porque, apesar de resolver bem escritores concorrentes,
ainda exige um serviço rodando e um processo de backup mais elaborado que "copiar um arquivo" — e o
ganho de múltiplos escritores não se traduz em benefício real na escala de uma PME, como o
[ADR-0012](0012-escritor-unico-com-group-commit.md) mede em números.

## Decisão

**SQLite embarcado é o motor de armazenamento padrão de produção, com um único escritor e um pool de
leitores por conexão WAL.**

A configuração está descrita em detalhe no [doc 07](../07-persistencia-sqlite.md):
`journal_mode=wal`, `synchronous=FULL` no escritor (durabilidade total a cada commit), todas as
tabelas de negócio declaradas `STRICT` (tipagem real, não a tipagem dinâmica padrão do SQLite). A
porta `PortaArmazenamento` ([doc 01 §5](../01-arquitetura-geral.md)) abstrai o acesso, e mantemos a
opção de um adaptador PostgreSQL para redes muito grandes — mas isso **não é objetivo da v1** e não
deve influenciar decisões de design hoje. O "gargalo" de um único escritor é, na prática, a maior
simplificação arquitetural do sistema, não uma limitação a contornar: ver
[ADR-0012](0012-escritor-unico-com-group-commit.md) para os números.

## Consequências

### Positivas

- Instalação zero: o banco é um arquivo, o cliente não vê "banco de dados" como conceito separado do
  próprio aplicativo.
- RAM de repouso ~60× menor que a alternativa cliente-servidor mais leve considerada (PostgreSQL).
- Backup é copiar um arquivo (mais o WAL); restauração é copiar de volta — sem `pg_dump`, sem agente
  de backup, sem agendamento externo a configurar.
- Trocar o PC que hospeda o servidor é copiar o arquivo `.db` para a máquina nova — não há
  reinstalação de serviço nem restauração de dump.
- Group commit (ver [ADR-0012](0012-escritor-unico-com-group-commit.md)) faz o único escritor sustentar
  78,3 vendas/s em HDD 5400 rpm e 2.900 vendas/s em SSD NVMe — duas ordens de grandeza acima do pico
  real de qualquer PME.

### Negativas

- Um único escritor é, por design, um teto de throughput de escrita — adequado à escala do produto,
  mas categoricamente inadequado para uma rede de 200 terminais ou um data warehouse transacional.
- Sem replicação nativa: alta disponibilidade multi-servidor (failover automático) não existe sem
  construir uma solução própria por cima.
- SQLite tem tipagem dinâmica por padrão — mitigada pelo modo `STRICT`, mas ainda assim menos rígida
  que a checagem de esquema de um SGBD cliente-servidor maduro.
- Limite prático de tamanho de base confortável: acima de ~20 GB, consultas históricas completas
  começam a pesar no cache de página compartilhado com a operação do dia a dia.
- Nenhuma ferramenta de administração gráfica com o polimento de um `pgAdmin`; inspeção manual é via
  CLI (`sqlite3`) ou ferramentas de terceiros de qualidade variável.

### Mitigações

- A porta `PortaArmazenamento` mantém viva a opção de um backend PostgreSQL para o cenário de
  multi-loja em grande escala, sem exigir reescrever nenhum módulo — é uma porta, não um acoplamento
  direto ([doc 01 §5](../01-arquitetura-geral.md)).
- Group commit resolve o teto de throughput na escala que importa: o pico real de uma PME é da ordem
  de dezenas de vendas por minuto, não por segundo.
- O plano de arquivamento de exercícios encerrados ([ADR-0011](0011-arquivamento-de-exercicios.md))
  resolve o limite prático de tamanho antes que ele vire problema de desempenho.
- `PRAGMA integrity_check` no boot, backup verificado automaticamente (restaurado e conferido contra
  a prova do razão) e checkpoint agressivo em bateria compensam a ausência de um DBA dedicado
  cuidando da saúde do banco ([doc 03 §5](../03-pilar-resiliencia.md)).

## Quando revisitar

- Se um cliente real precisar operacionalmente de mais de ~30 terminais simultâneos apontando para o
  mesmo servidor, ou de alta disponibilidade com failover automático de banco — cenário fora do
  escopo de v1, mas coberto pela porta `PortaArmazenamento`.
- Se a fila do escritor único, medida em produção (métrica de profundidade de fila do
  [ADR-0012](0012-escritor-unico-com-group-commit.md)), ultrapassar sistematicamente 100 ms de espera
  em horário de pico — hoje isso exigiria uma carga cerca de 30× acima do cenário sintético de
  referência (`supermercado_bairro`, 3 PDVs, 1.200 cupons/dia).
- Se a base operacional de um cliente único ultrapassar o teto de arquivamento de 20 GB
  ([ADR-0011](0011-arquivamento-de-exercicios.md)) e o arquivamento não for suficiente para manter o
  desempenho de consulta corrente dentro do orçamento do [doc 02](../02-pilar-eficiencia.md).
- Se o roadmap do Fase 6 ([doc 17](../17-roadmap.md)) — multi-loja com consolidação central e
  servidor por filial — exigir replicação em tempo real entre servidores, hoje inexistente no SQLite
  sem solução de terceiros.
