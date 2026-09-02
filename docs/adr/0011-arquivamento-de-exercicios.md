# ADR-0011 — Arquivar exercícios encerrados em bases anexadas somente leitura

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0003, ADR-0012

## Contexto

O SQLite embarcado ([ADR-0003](0003-sqlite-como-motor-de-armazenamento.md)) é confortável para o
tamanho de base de uma PME típica — cerca de 2,4 GB para um comércio médio depois de três anos de
operação ([doc 06 §6](../06-modelo-de-dados.md)). Mas o histórico de um ERP cresce indefinidamente: a
guarda fiscal obrigatória no Brasil é de cinco anos ([doc 03 §5.1](../03-pilar-resiliencia.md)), e um
cliente de sucesso, operando por uma década, acumula uma base muito maior do que os três anos usados
como referência de dimensionamento.

O sintoma que essa decisão evita é concreto: conforme a base cresce, o cache de páginas compartilhado
entre a operação do dia a dia e consultas que varrem anos de histórico ("compare este mês com o mesmo
mês há 4 anos") passa a competir pelo mesmo recurso. Uma consulta histórica pesada em hora de pico
degrada a experiência do PDV — exatamente o tipo de acoplamento indevido entre "operação" e "análise"
que o [ADR-0007](0007-duckdb-e-parquet-para-analitico.md) já resolve para o caso analítico, mas que
reaparece aqui na forma de "base operacional grande demais para o hardware modesto do cliente".

Ao mesmo tempo, nada pode ser descartado: "nada é apagado" é princípio de produto
([doc 00 §4](../00-visao-produto.md)) e a guarda fiscal de cinco anos é uma obrigação legal, não uma
preferência de arquitetura. Expurgo — simplesmente apagar dados antigos — está fora de cogitação. A
pergunta é apenas *onde* o dado antigo mora fisicamente, não se ele continua existindo e acessível.

## Alternativas consideradas

| Critério | **Base anexada por exercício (`ATTACH`)** | Particionamento por tabela (sharding manual) | Expurgo de dados antigos | Mover histórico para outro motor (ex.: DuckDB) |
|---|---|---|---|---|
| Cumpre guarda fiscal de 5 anos | sim — dado preservado, só migra de arquivo | sim | **não permitido** — proibido por lei | sim, mas fora do modelo relacional operacional |
| Impacto no cache de página da base corrente | baixo — exercício fechado sai do arquivo ativo | médio — ainda no mesmo arquivo físico | nenhum (mas ilegal) | baixo |
| Transparência para o módulo consumidor | alta — `UNION ALL` sobre as duas bases, mesma interface SQL | média — a lógica de módulo precisa saber em qual partição buscar | não aplicável | baixa — exigiria trocar o motor de consulta para histórico |
| Complexidade de implementação | média (gerenciar `ATTACH`, hash de selo) | alta (reescrever toda consulta cross-partição) | trivial, mas ilegal | alta (dois motores de consulta coexistindo para o mesmo dado) |
| Custo de backup | um arquivo a mais por exercício fechado | um arquivo só, mas maior | um arquivo, menor (mas ilegal) | dois formatos diferentes a versionar |
| Prova de integridade do exercício fechado | sim — hash selado por exercício | possível, mas não isolado por arquivo | não aplicável | possível, mas fora do modelo transacional do razão |

Particionamento manual por tabela foi a alternativa mais próxima de ser aceita, mas foi descartada
porque exigiria reescrever toda consulta que cruza exercícios (comparativo ano a ano, por exemplo) para
saber explicitamente em qual partição procurar — o `ATTACH` do SQLite entrega a mesma transparência de
consulta (`UNION ALL`) sem exigir essa reescrita, porque o SQL não muda, apenas o plano de execução.

## Decisão

**Quando a base ultrapassa ~20 GB, exercícios encerrados (anos com o período fechado,
[doc 05 §7](../05-nucleo-financeiro.md)) são migrados para um arquivo separado por ano
(`cardeal-2024.db`), anexado à conexão como somente leitura; consultas que precisam de histórico usam
`UNION ALL` transparente entre a base corrente e as anexadas, e cada exercício arquivado carrega um
hash selado que prova que não foi alterado após o arquivamento.**

O gatilho de tamanho (~20 GB) e o mecanismo estão descritos no
[doc 07 §9](../07-persistencia-sqlite.md). A migração só é possível para exercícios já fechados
([doc 05 §7](../05-nucleo-financeiro.md)) — um período fechado já não aceita novos lançamentos nem
estornos sem reabertura explícita, o que torna seguro movê-lo para um arquivo somente leitura sem
risco de uma escrita tardia ser perdida. O hash selado do exercício (o mesmo mecanismo do
`hash_saldos` do fechamento de período, [doc 05 §7](../05-nucleo-financeiro.md)) permite provar, a
qualquer momento, que o arquivo arquivado não foi adulterado depois de selado — inclusive por fora do
sistema.

## Consequências

### Positivas

- A base operacional corrente permanece pequena e rápida indefinidamente, independentemente de quanto
  histórico o cliente acumula ao longo dos anos — o desempenho do PDV não degrada com o tempo de uso.
- Nenhum dado é perdido nem descartado: a guarda fiscal de cinco anos (e, na prática, indefinida, já
  que nada é apagado por princípio de produto) é cumprida mantendo o arquivo completo, só realocado.
- Consultas históricas continuam possíveis com o mesmo SQL, sem lógica especial no módulo consumidor
  — o `UNION ALL` entre bases é transparente à camada de domínio.
- Cada exercício arquivado é auditável de forma independente: o hash selado prova integridade sem
  depender da base corrente estar íntegra.
- O tamanho de backup incremental diário cai, porque exercícios arquivados são somente leitura e não
  mudam — só a base corrente precisa de backup completo frequente.

### Negativas

- Consultas que cruzam vários exercícios (por exemplo, um comparativo de cinco anos) ficam mais lentas
  que uma varredura de tabela única, porque o SQLite precisa combinar planos de execução de múltiplos
  arquivos anexados via `UNION ALL`.
- A lógica de consulta cross-exercício é mais complexa de escrever e de otimizar do que uma consulta
  sobre uma única tabela — exige atenção a índices espelhados em cada base arquivada.
- Backup passa a envolver múltiplos arquivos em vez de um só — a rotina de backup e restauração
  ([doc 03 §5](../03-pilar-resiliencia.md)) precisa versionar e verificar cada exercício arquivado
  separadamente, não apenas o arquivo corrente.
- Restaurar o sistema em uma máquina nova exige copiar não apenas `cardeal.db`, mas todos os arquivos
  de exercícios anexados relevantes — um passo a mais no procedimento de recuperação de desastre.

### Mitigações

- O gatilho de 20 GB é generoso: para o cliente típico (~2,4 GB em três anos,
  [doc 06 §6](../06-modelo-de-dados.md)), o arquivamento só se torna relevante depois de mais de uma
  década de operação contínua — a maioria dos clientes nunca vai sentir esse mecanismo ativo.
- Índices são recriados de forma equivalente em cada base arquivada no momento da migração, mantendo
  o plano de consulta de cada arquivo individual eficiente mesmo quando combinado via `UNION ALL`.
- A rotina de backup diário ([doc 03 §5.1](../03-pilar-resiliencia.md)) trata cada exercício arquivado
  como um artefato imutável selado por hash: uma vez verificado e arquivado, o backup daquele arquivo
  não precisa se repetir a cada ciclo, só confirmar que o hash continua batendo.
- A migração de um exercício para arquivo separado é, ela mesma, uma operação testada e verificada
  (hash antes e depois, prova do razão sobre o conjunto combinado) antes de considerar o exercício
  seguramente arquivado.

## Quando revisitar

- Se, na prática, uma parcela relevante de clientes atingir o gatilho de 20 GB muito antes do esperado
  (por exemplo, dentro de três a quatro anos de uso, não uma década) — sinal de que o gatilho está
  calibrado errado ou que o perfil de uso real gera mais dado por período do que o cenário de
  referência assume.
- Se consultas cross-exercício se tornarem uma necessidade frequente de uso real (não só relatórios
  anuais ocasionais) e a degradação de desempenho do `UNION ALL` multi-arquivo se tornar perceptível
  o suficiente para incomodar o usuário — nesse caso, valeria reconsiderar mover o histórico para o
  DuckDB analítico ([ADR-0007](0007-duckdb-e-parquet-para-analitico.md)) em vez de manter arquivos
  SQLite anexados.
- Se o procedimento de backup e restauração com múltiplos arquivos se provar, em incidentes reais,
  fonte recorrente de erro operacional (por exemplo, um cliente restaura só o arquivo corrente e perde
  acesso ao histórico arquivado sem perceber) — sinal de que o processo de restauração precisa de mais
  automação ou de um formato de empacotamento único.
