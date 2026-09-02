# ADR-0007 — DuckDB + Parquet para a carga analítica

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0003, ADR-0012

## Contexto

O razão gerencial é, por construção, uma tabela fato já normalizada — competência, conta, contraparte,
centro de custo, quantidade e valor em cada partida ([doc 05 §10](../05-nucleo-financeiro.md)). Isso
significa que perguntas de análise (curva ABC, margem real por produto, cesta de compras, previsão de
fluxo de caixa) são, tecnicamente, apenas agregações SQL sobre essa tabela. A tentação óbvia é
consultar o próprio SQLite operacional para responder essas perguntas.

O problema é de contenção, não de correção do resultado. Uma agregação sobre três anos de vendas por
categoria varre milhões de linhas de `razao_partida` e polui o cache de páginas que a operação de
venda precisa quente o tempo todo. Rodar isso em hora de pico faz o PDV lentificar — e "vender nunca
pode parar" ([doc 00 §4](../00-visao-produto.md)) é um princípio de produto, não uma preferência. O
[doc 11 §1](../11-analytics-bi.md) chama isso de "analisar nunca atrapalha vender", e trata como
requisito de escalonamento, não como slogan.

Ao mesmo tempo, dados analíticos crescem em um padrão de acesso diferente do operacional: leitura em
massa, colunar por natureza (somar uma coluna inteira, agrupar por poucas dimensões), com tolerância a
alguns segundos de atraso em troca de agregações muito mais rápidas. Esse é exatamente o caso de uso
para o qual bancos colunares embarcados (DuckDB) e formatos de armazenamento colunar comprimido
(Parquet) foram desenhados — e, tal como o SQLite, ambos rodam embutidos, sem servidor a administrar,
coerente com o restante da filosofia operacional do produto ([ADR-0003](0003-sqlite-como-motor-de-armazenamento.md)).

## Alternativas consideradas

| Critério | **DuckDB + Parquet** | Consultar o SQLite operacional | ClickHouse | Data warehouse na nuvem | Materialized views no SQLite |
|---|---|---|---|---|---|
| Isola OLAP de OLTP | sim, processo/arquivo separado | não — mesma conexão, mesmo cache de página | sim | sim | não — mesma base |
| Instalação/operação | zero (embutido) | zero (já existe) | serviço a administrar | conta na nuvem, custo recorrente, latência de rede | zero, mas lógica de trigger a manter |
| Velocidade de agregação em milhões de linhas | muito alta (colunar) | baixa-média (linha a linha) | muito alta | alta, mas com latência de rede | média (índices ajudam, mas ainda é linha a linha) |
| Formato de dado portável para o cliente | sim (Parquet é aberto) | não diretamente (exigiria exportar) | não diretamente | geralmente proprietário ou exportável com esforço | não |
| Funciona offline (sem internet) | sim | sim | sim, se auto-hospedado | não (depende do provedor) | sim |
| Risco de atrapalhar a operação em hora de pico | nenhum (processo/prioridade separados) | alto (mesmo cache de página do OLTP) | baixo, mas exige infraestrutura própria | nenhum (isolado por natureza), mas custo de rede | alto (triggers custam no caminho de escrita) |
| Custo de manutenção contínua | baixo (biblioteca embutida) | nenhum adicional | alto (cluster, backup, upgrade) | alto (custo recorrente + gestão de acesso) | médio (triggers são difíceis de testar, proibidos pelo [doc 07 §10](../07-persistencia-sqlite.md)) |

ClickHouse e um data warehouse na nuvem foram descartados principalmente por exigirem infraestrutura
que contraria a filosofia "zero instalação" do produto ([doc 00 §6](../00-visao-produto.md)): o
cliente de uma PME não vai administrar um cluster OLAP, e depender de nuvem contraria o princípio de
que a operação (inclusive analisar) precisa funcionar sem internet. Materialized views via trigger
foram descartadas de saída porque triggers são proibidos no projeto por serem lógica invisível e
difícil de testar isoladamente ([doc 07 §10](../07-persistencia-sqlite.md)).

## Decisão

**A carga analítica é servida por DuckDB consultando Parquet particionado, alimentado por um
projetor que lê o outbox do razão em prioridade baixa — nunca pelo SQLite operacional diretamente.**

A arquitetura, descrita no [doc 11](../11-analytics-bi.md), separa fisicamente OLTP de OLAP: o
projetor consome o outbox transacional ([doc 06 §2](../06-modelo-de-dados.md)) e escreve arquivos
Parquet particionados por empresa/ano/mês, que o DuckDB lê somente leitura para alimentar as telas de
análise e exportações. O esquema é estrela (fatos `fato_partida`, `fato_venda_item`,
`fato_movimento_estoque`; dimensões `dim_tempo`, `dim_produto`, `dim_cliente` com SCD tipo 2 — ver
[doc 11 §2](../11-analytics-bi.md)). O projetor roda em prioridade de thread abaixo do normal e cede
CPU sob carga: "analisar nunca atrapalha vender" é escalonamento, não promessa.

## Consequências

### Positivas

- Agregações colunares são medidas em ordens de grandeza mais rápidas que a varredura linha a linha
  equivalente no SQLite — por exemplo, DRE comparativo de 24 meses sobre 13 milhões de partidas em
  menos de 250 ms ([doc 11 §7](../11-analytics-bi.md)).
- A operação (vendas, baixas, lançamentos) nunca compete por cache de página ou I/O com uma consulta
  analítica pesada — os dois mundos são fisicamente separados.
- Parquet é um formato aberto: o cliente pode levar seus próprios dados para qualquer ferramenta
  (Excel, Power BI, pandas, polars) sem depender do Cardeal para exportar — portabilidade total do
  dado do cliente ([doc 11 §5](../11-analytics-bi.md)).
- Compressão de 8 a 12× reduz significativamente o espaço em disco comparado ao dado bruto,
  parcialmente compensando a duplicação inerente ao modelo.
- Ambos os motores são embutidos, sem servidor — coerente com a filosofia de instalação zero do
  restante do sistema ([ADR-0003](0003-sqlite-como-motor-de-armazenamento.md)).
- Reconstrução total é sempre possível a partir do razão (`cargo xtask analytics --reconstruir`) —
  apagar o Parquet nunca perde dado, o que dá liberdade para evoluir o esquema analítico sem medo.

### Negativas

- Dado analítico tem atraso real em relação ao operacional: cerca de 5 segundos para o mês corrente
  ([doc 11 §3](../11-analytics-bi.md)) — quem quer o número exato do último segundo precisa ir ao
  razão diretamente, não ao analítico.
- Espaço em disco duplicado: os mesmos fatos existem no SQLite operacional (formato linha) e no
  Parquet (formato colunar comprimido) — mais disco usado para o mesmo dado, ainda que comprimido.
- Mais um motor de execução embutido no binário (DuckDB, além de SQLite) — mais superfície de
  dependência, mais peso no binário final, mais uma tecnologia para a equipe conhecer.
- Um projetor em atraso (por exemplo, depois de um pico de carga sustentado) pode deixar o mês
  corrente visivelmente desatualizado na tela de análise até se recuperar — o produto precisa
  comunicar isso, não apenas escondê-lo.

### Mitigações

- Como o Parquet é inteiramente derivado e reconstruível, qualquer divergência ou corrupção se
  resolve com `cargo xtask analytics --reconstruir` — sem risco de perda de dado, porque a fonte de
  verdade continua sendo o razão.
- Particionamento por empresa/ano/mês permite *partition pruning*: uma consulta de um trimestre lê
  três arquivos, não a base inteira, mantendo o custo proporcional ao período perguntado, não ao
  histórico total ([doc 11 §2](../11-analytics-bi.md)).
- O projetor roda em prioridade de thread baixa e cede CPU ativamente quando o motor está sob carga de
  operação — o atraso do mês corrente cresce sob pico, mas a operação nunca é penalizada em troca
  ([doc 11 §3](../11-analytics-bi.md)).
- A tela de análise mostra explicitamente de onde vêm os números e permite "ver o SQL gerado"
  ([doc 11 §6](../11-analytics-bi.md)) — reduzindo o risco de um usuário interpretar um número
  levemente atrasado como definitivo sem entender a latência.

## Quando revisitar

- Se a latência de atualização do mês corrente ultrapassar minutos de forma sustentada em uso real
  (hoje a meta é ~5 s), a ponto de o dono perceber divergência visível entre o Pulso (que lê o razão
  direto) e as telas de análise (que leem o Parquet) na mesma sessão de trabalho.
- Se o espaço em disco duplicado se tornar proibitivo em instalações de disco pequeno (por exemplo,
  terminais com SSD de 128 GB) antes mesmo do gatilho de arquivamento de exercícios
  ([ADR-0011](0011-arquivamento-de-exercicios.md)).
- Se o tempo de reconstrução total (`cargo xtask analytics --reconstruir`, hoje com meta de menos de
  4 minutos para 13 milhões de partidas, [doc 11 §7](../11-analytics-bi.md)) crescer a ponto de se
  tornar impraticável em uma janela de manutenção razoável para bases maiores.
- Se o DuckDB parar de evoluir suporte a uma funcionalidade que se torne crítica (por exemplo,
  escrita concorrente multi-processo, hoje fora do escopo de uso) exigida por um requisito de produto
  futuro.
