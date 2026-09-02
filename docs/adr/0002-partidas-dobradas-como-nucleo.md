# ADR-0002 — Partidas dobradas como núcleo de integração

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0005, ADR-0012, ADR-0013

## Contexto

Todo ERP de PME resolve, cedo ou tarde, o mesmo problema: como fazer o PDV, o estoque, as contas a
pagar e o financeiro concordarem sobre quanto dinheiro a empresa tem. A resposta mais comum no
mercado é uma tabela `movimento_caixa` com um campo `valor` que soma positivo e negativo — simples de
implementar, mas simples de deixar divergir. Um bug em um módulo grava um movimento sem contrapartida,
e seis meses depois o contador descobre que o saldo do sistema não bate com o extrato do banco. Não
há como provar, em tempo de execução, que a base está consistente — só reconciliar manualmente.

O Cardeal nasce de uma tese de produto explícita ([doc 00 §2](../00-visao-produto.md)): o financeiro
não é um módulo que recebe dados dos outros, é o substrato em que os outros escrevem, na mesma
transação. Isso só funciona se existir um mecanismo de escrita que seja, ao mesmo tempo, universal
(serve venda, aluguel, ordem de serviço, abastecimento) e auto-verificável (um bug aparece como
desbalanceamento, não como relatório errado). Partidas dobradas — o método usado pela contabilidade
há setecentos anos — é exatamente isso: todo lançamento tem débito e crédito de igual valor, e a soma
de tudo é sempre zero por construção.

A alternativa de módulos com saldo próprio mais um job de consolidação noturna foi descartada cedo,
porque reintroduz exatamente o problema que queremos eliminar: duas verdades (o saldo do módulo e o
saldo consolidado) que podem divergir entre a consolidação e a próxima. Event sourcing genérico sobre
todos os agregados do sistema também foi avaliado e descartado como escopo geral — ver
[ADR-0005](0005-event-sourcing-restrito-ao-razao.md) — porque a maioria dos agregados (cadastro de
cliente, produto) não precisa da capacidade de "replay" que o razão precisa, e pagaria o custo de
complexidade de projeção sem ganho correspondente.

Um argumento adicional, mais silencioso: a exportação fiscal e contábil (ECD/SPED, via
[doc 10](../10-modulo-fiscal.md)) já fala a língua de partidas dobradas. Escolher qualquer outro
modelo interno significaria manter uma tradução permanente entre "como guardamos" e "como o contador
enxerga" — e cada tradução é um lugar a mais onde um bug pode se esconder.

## Alternativas consideradas

| Critério | **Partidas dobradas** | Tabela de movimento com sinal | Saldos por módulo + consolidação | Event sourcing genérico |
|---|---|---|---|---|
| Invariante verificável continuamente | sim (`Σ partidas = 0`) | não (nada impede um lançamento sem contrapartida) | não (depende do job rodar certo) | depende de replay correto de todo agregado |
| Uma consulta serve todo relatório (DRE, fluxo, margem) | sim | não (cada relatório é uma tabela nova) | não (cada módulo expõe sua própria forma) | sim, mas com custo de projeção alto |
| Rastreabilidade "de onde veio esse real" | nativa (contraparte + origem) | manual | manual, entre sistemas | nativa, mas cara de consultar |
| Alinhamento com exportação contábil (ECD/SPED) | direto | requer tradução | requer tradução | requer tradução |
| Curva de aprendizado da equipe | alta (vocabulário contábil) | baixa | média | alta (arquitetura + domínio) |
| Custo de armazenamento por operação | ~4 partidas por venda | 1 linha por operação | 1 linha por módulo | N eventos por agregado |
| Precedente de mercado em ERP de PME | incomum (ERPs grandes usam; PME raramente) | comum | comum | raro |

## Decisão

**Toda operação de negócio que move valor grava, na mesma transação, um lançamento de partidas
dobradas no Razão gerencial — não existe caminho alternativo para registrar efeito financeiro.**

O núcleo é o crate `cardeal-ledger`, descrito em detalhe no [doc 05](../05-nucleo-financeiro.md).
Cada `Lancamento` carrega duas ou mais `Partida`s cujo campo `valor` (positivo para débito, negativo
para crédito) soma exatamente zero; o tipo `LancamentoBalanceado` só existe se essa soma fechar — o
invariante é impossível de violar, não apenas validado (ver [ADR-0005](0005-event-sourcing-restrito-ao-razao.md)
para o motivo de o razão ser o único lugar do sistema com esse desenho de append-only). O
[receituário](../05-nucleo-financeiro.md#5-o-receituário-como-cada-módulo-posta) é o contrato público:
toda nova funcionalidade que move dinheiro declara sua linha ali e prova com teste
(`assert_lancamento!`) antes de ser aceita em revisão.

## Consequências

### Positivas

- Um bug de integração vira desbalanceamento detectável imediatamente pela prova do razão
  ([doc 03 §5.3](../03-pilar-resiliencia.md)), não um relatório errado descoberto meses depois.
- DRE, fluxo de caixa, margem por produto, saldo de cliente e valor de estoque são a mesma consulta
  com filtros diferentes sobre `razao_partida` — nenhuma tabela nova por relatório novo.
- Estoque, crédito de cliente e dívida com fornecedor compartilham o mesmo mecanismo de saldo — não
  há três subsistemas a manter sincronizados.
- A exportação para ECD/SPED é um mapeamento `de → para`, não uma reconstrução de dados a partir de
  fontes heterogêneas.
- O razão é, por construção, uma tabela fato já normalizada — o [doc 11](../11-analytics-bi.md) parte
  dela sem transformação para alimentar o analítico.

### Negativas

- Curva de aprendizado contábil real para a equipe: um desenvolvedor sem histórico em contabilidade
  precisa entender débito/crédito, natureza de conta e regime de competência antes de contribuir com
  segurança em qualquer módulo que movimente dinheiro.
- Mais linhas gravadas por operação: uma venda simples no PDV grava tipicamente 4 partidas (caixa,
  receita, CMV, estoque) em vez de 1 movimento único — mais I/O por operação de negócio, ainda que
  dentro da mesma transação e do mesmo `fsync` (ver [ADR-0012](0012-escritor-unico-com-group-commit.md)).
- Um lançamento malformado por um módulo novo (conta errada, natureza trocada) não é pego pelo tipo
  — o compilador garante balanceamento, não *correção* contábil. Isso só é pego por teste ou por
  revisão humana da linha do receituário.
- Onboarding mais longo para quem só quer "adicionar uma feature" sem tocar em dinheiro: mesmo assim
  precisa entender o vocabulário para não errar a contrapartida.

### Mitigações

- `ConstrutorLancamento` é o único caminho para persistir um lançamento e é validado por tipo — o
  desbalanceamento é impossível de compilar, não apenas detectado em teste
  ([doc 05 §3.3](../05-nucleo-financeiro.md)).
- O [receituário](../05-nucleo-financeiro.md#5-o-receituário-como-cada-módulo-posta) é documentado e
  testado por `assert_lancamento!`: todo módulo novo escreve sua linha e o teste correspondente antes
  de o PR ser aceito — isso não é opcional, é portão de revisão ([doc 14 §4](../14-testes-qualidade.md)).
- O onboarding do [doc 16](../16-onboarding.md) usa o caminho de uma venda como primeiro exercício
  guiado, exatamente para construir o vocabulário contábil na prática, não na teoria.
- Testes de propriedade (`proptest`) provam que nenhuma sequência de operações produz lançamento
  desbalanceado, cobrindo casos que a revisão humana não pensaria em testar
  ([doc 14 §3](../14-testes-qualidade.md)).

## Quando revisitar

- Se, medido por telemetria de erro em revisão, mais de 20% dos PRs de módulos novos precisarem de
  correção de receituário depois de já terem passado no teste automatizado — sinal de que o modelo
  mental não está sendo internalizado apesar das mitigações.
- Se o overhead de armazenamento de partidas (hoje ~1,1 GB para 13 milhões de partidas em três anos
  de um comércio médio, [doc 06 §6](../06-modelo-de-dados.md)) se tornar o gargalo dominante de disco
  em instalações típicas, antes mesmo do gatilho de arquivamento de 20 GB do
  [ADR-0011](0011-arquivamento-de-exercicios.md).
- Se um perfil de empresa completamente novo (por exemplo, um modelo de negócio sem contrapartida
  financeira clara) não conseguir ser expresso naturalmente em partidas dobradas — sinal de que o
  modelo alcançou seu limite de generalidade.
- Não revisitaríamos essa decisão por "é difícil para gente nova entender rápido" isoladamente: esse
  é um custo conhecido, pago uma vez por pessoa, e a mitigação de onboarding já assume esse custo
  como parte do design.
