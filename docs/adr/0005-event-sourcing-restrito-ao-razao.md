# ADR-0005 — Event sourcing restrito ao razão, CRUD auditado no resto

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0002, ADR-0012

## Contexto

O [doc 01 §4](../01-arquitetura-geral.md) já declara a posição do produto: "separamos comando e
consulta, mas sem event sourcing generalizado e sem bancos separados — essa é a dose de CQRS que se
paga". Esta ADR registra formalmente por que essa dose é exatamente essa, e não maior nem menor.

O Razão é, por natureza contábil, um livro de fatos que só se acumulam: um lançamento não é corrigido
por `UPDATE`, é anulado por um lançamento de estorno que o referencia ([doc 05 §6](../05-nucleo-financeiro.md)).
Essa regra não é uma escolha de arquitetura — é uma exigência do domínio (e, em parte, da legislação
fiscal, que exige rastreabilidade de correções). O razão já é, por definição, "event sourced": cada
lançamento é um evento imutável, e o saldo de qualquer conta é a soma dos eventos até uma data —
exatamente o padrão de projeção que o event sourcing formaliza.

O resto do sistema é diferente. Um cadastro de cliente, um produto no catálogo, uma configuração de
sistema — são entidades com estado atual que muda ao longo do tempo, mas cuja história completa de
mudança não tem valor de negócio comparável ao da história do dinheiro. Modelar "o cliente mudou de
telefone três vezes em 2025" como uma sequência de eventos reproduzíveis paga um custo real de
projeção (materializar o estado atual a partir do histórico, versionar o esquema de eventos ao longo
dos anos) sem que ninguém no produto precise responder "qual era o telefone do cliente em qualquer
ponto do passado" com a mesma urgência que precisa responder "qual era o saldo de caixa em qualquer
ponto do passado".

Adotar event sourcing em todos os agregados do sistema — a alternativa mais "pura" do ponto de vista
arquitetural — foi avaliada e descartada por essa razão prática: o ganho de auditabilidade completa
já é entregue, para o CRUD, por um mecanismo mais barato — a auditoria encadeada por hash
([doc 06 §2.1](../06-modelo-de-dados.md)) — sem pagar o custo de reconstrução de estado a partir de
eventos para entidades que não precisam dele.

## Alternativas consideradas

| Critério | **ES restrito ao razão** | Event sourcing completo (todos os agregados) | CRUD puro em tudo (sem ES em lugar nenhum) |
|---|---|---|---|
| Fidelidade ao domínio contábil (razão é intrinsecamente append-only) | alta — o modelo do razão já é assim por natureza | alta, mas redundante com o que o razão já entrega | baixa — exigiria `UPDATE`/`DELETE` em fatos financeiros, o que é proibido pelo produto |
| Complexidade de projeção fora do financeiro | baixa (CRUD direto) | alta (todo agregado precisa de projeção e, eventualmente, snapshot) | baixa |
| Auditabilidade do CRUD | alta (auditoria encadeada por hash, diff de campos) | alta, mas via replay completo de eventos | média (log de auditoria sem garantia de imutabilidade forte) |
| Curva de aprendizado da equipe | média (dois modelos mentais, mas cada um localizado) | alta (todo módulo novo exige pensar em eventos e projeções) | baixa |
| Custo de evolução de esquema | localizado ao razão (bem testado, estável) | espalhado por todos os módulos, cada um versionando seus próprios eventos internos | trivial (migração de coluna) |
| Consistência com o receituário e a exportação contábil | direta | indireta (eventos de negócio ≠ eventos contábeis) | não aplicável ao financeiro (não usaríamos CRUD ali de qualquer forma) |

## Decisão

**Event sourcing existe apenas no Razão (`cardeal-ledger`), onde o domínio já é intrinsecamente
append-only; todo o restante do sistema usa CRUD convencional com auditoria imutável encadeada por
hash.**

No razão, um `Lancamento` nunca é alterado depois de confirmado; correção é sempre um novo lançamento
de estorno vinculado ao original ([doc 05 §6](../05-nucleo-financeiro.md)). Fora do razão — cadastro
de cliente, produto, configuração, papéis de usuário — os módulos usam `UPDATE` convencional com
bloqueio otimista por `versao` ([doc 03 §4.1](../03-pilar-resiliencia.md)), e toda operação marcada
`audita` grava um registro em `nucleo_auditoria` com o diff de campos alterados, encadeado por hash
BLAKE3 ao registro anterior ([doc 06 §2.1](../06-modelo-de-dados.md)). A fronteira entre os dois
mundos é exatamente "isso afeta dinheiro?" — se sim, vira lançamento no razão
([doc 04 §5.1](../04-pilar-modularidade.md)); se não, é CRUD auditado.

## Consequências

### Positivas

- Cada módulo usa o modelo mais simples que resolve seu problema — não pagamos complexidade de
  projeção onde ela não compra nada.
- O razão continua sendo a única fonte de verdade para "o que aconteceu com o dinheiro e quando",
  sem competir com um sistema de eventos genérico que teria semântica diferente.
- A auditoria encadeada por hash entrega rastreabilidade forte ao lado CRUD (quem mudou o quê, quando,
  e prova de que a linha de auditoria não foi adulterada) sem o custo de manter snapshots e
  replay de eventos para entidades de baixo volume de mudança.
- Onboarding mais rápido para tarefas fora do financeiro: um desenvolvedor que só precisa adicionar um
  campo ao cadastro de cliente ([doc 16 — hora 3](../16-onboarding.md)) não precisa aprender event
  sourcing para fazer isso.

### Negativas

- Dois modelos mentais coexistem no mesmo sistema: um desenvolvedor precisa saber, para cada
  operação, se está no território "razão" (nunca `UPDATE`, sempre lançamento) ou no território "CRUD
  auditado" (`UPDATE` com versão, log de diff) — e escolher o errado é um erro de design que só a
  revisão pega, não o compilador.
- O histórico completo de mudança de uma entidade CRUD (por exemplo, todas as mudanças de limite de
  crédito de um cliente ao longo de cinco anos) é reconstruível a partir da auditoria, mas não é uma
  "linha do tempo de domínio" de primeira classe como seria em um agregado com event sourcing — exige
  consultar a tabela de auditoria e interpretar diffs, não reproduzir eventos de negócio nomeados.
- Migrações de esquema em tabelas CRUD (adicionar coluna, por exemplo) são mais diretas que evoluir um
  esquema de eventos, mas ainda assim exigem cuidado quando a auditoria referencia a estrutura antiga
  de um registro já alterado.

### Mitigações

- A fronteira é documentada de forma explícita e verificável: "todo efeito financeiro passa pelo
  razão; não existe tabela de saldo paralela" é regra inviolável de módulo
  ([doc 04 §8](../04-pilar-modularidade.md)), e revisão de PR confere isso na prática.
- A tabela de auditoria (`nucleo_auditoria`) é, ela mesma, apenas gravada — nunca alterada — e sua
  cadeia de hash é verificada diariamente, dando ao lado CRUD uma garantia de integridade equivalente,
  em espírito, à do razão, sem replay ([doc 08 §5](../08-seguranca-permissoes.md)).
- O [doc 16](../16-onboarding.md) ensina a fronteira explicitamente no primeiro dia — "se é regra pura
  de negócio → domínio; se é efeito financeiro → razão; se é cadastro → CRUD auditado" é uma das
  primeiras perguntas respondidas no guia de onboarding.
- Revisão de PR trata "isso deveria ser lançamento ou CRUD?" como uma pergunta padrão de checklist
  para qualquer comando novo que toque em valor monetário, mesmo que indiretamente — reduzindo a
  chance de a dúvida só aparecer depois que o módulo já está em produção.

## Quando revisitar

- Se um módulo fora do financeiro passar a precisar, de forma recorrente e com pedido explícito de
  clientes reais, de "linha do tempo de domínio" reproduzível (por exemplo, reconstruir o estado
  completo de uma negociação de CRM em qualquer ponto do passado) — sinal de que aquele agregado
  específico se beneficiaria de event sourcing local, sem generalizar para todo o sistema.
- Se a auditoria encadeada por hash não se provar suficiente para alguma exigência regulatória nova
  (por exemplo, uma obrigação legal que exija replay formal de eventos, não apenas prova de
  integridade de log).
- Se o volume de mudanças em uma entidade específica crescer a ponto de a tabela de auditoria
  associada se tornar, ela mesma, o gargalo de armazenamento antes do razão — sinal de que aquele caso
  particular merece um desenho dedicado.
- Não revisitaríamos essa decisão pela dificuldade de "ter dois modelos mentais": essa dualidade é
  deliberada e localizada — o critério de decisão (afeta dinheiro? razão. não afeta? CRUD) é simples
  o suficiente para não gerar ambiguidade recorrente em revisão.
