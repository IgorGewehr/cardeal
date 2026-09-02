# ADR-0006 — Delegar emissão fiscal a uma API externa

- **Status:** Aceita
- **Data:** 2026-09-01
- **Decisores:** Equipe de arquitetura
- **Relacionadas:** ADR-0014

## Contexto

O sistema tributário brasileiro é, por si só, um domínio de complexidade comparável ao ERP inteiro.
Emitir um documento fiscal eletrônico corretamente exige: assinatura digital com certificado
ICP-Brasil, comunicação SOAP com 27 SEFAZs estaduais diferentes (cada uma com particularidades),
acompanhamento de notas técnicas que mudam praticamente todo mês, validação contra esquemas XSD que
também mudam, contingência formal (SVC-AN/SVC-RS) para quando a SEFAZ de origem está fora do ar, e a
geração de obrigações acessórias (SPED Fiscal, SPED Contribuições, ECD, ECF, EFD-Reinf). Do lado
municipal, NFS-e sozinha tem cerca de 5.570 padrões diferentes — um por município
([doc 10 §1](../10-modulo-fiscal.md)).

Isso não é uma funcionalidade do ERP: é, por complexidade e por superfície de manutenção contínua, um
produto à parte. Empresas que se especializam só nisso (Focus NFe, NFe.io, Tecnospeed, PlugNotas,
WebmaniaBR) mantêm equipes dedicadas a acompanhar exatamente essa superfície móvel, com SLA e
responsabilidade contratual sobre a conformidade fiscal.

Ao mesmo tempo, o princípio de produto "vender nunca pode parar" ([doc 00 §4](../00-visao-produto.md))
é absoluto e não pode ser condicionado à disponibilidade de nenhum serviço externo, fiscal incluído.
O modelo de negócio do Cardeal já está desenhado para que o fiscal seja um agregado separado, com
ciclo de vida próprio e assíncrono em relação à venda ([doc 10 §3](../10-modulo-fiscal.md)) — o que
torna a escolha de quem implementa o emissor uma decisão isolável do resto da arquitetura, desde que
o contrato entre os dois lados seja bem definido.

A alternativa de implementar um emissor fiscal próprio, ou de usar uma biblioteca de código aberto
como o ACBr (referência no mercado brasileiro, escrita majoritariamente em Object Pascal/Delphi) via
FFI, foi considerada e descartada — não por incapacidade técnica, mas porque manter essa superfície
atualizada mês a mês contra 27 SEFAZs e 5.570 municípios desviaria a maior parte do esforço de
engenharia do produto para um domínio que não é o nosso.

## Alternativas consideradas

| Critério | **API fiscal externa** | Emissor próprio | ACBr via FFI | SDK comercial embarcado |
|---|---|---|---|---|
| Esforço de manutenção contínua (notas técnicas, XSDs) | do provedor | nosso, permanente | nosso (adaptar ACBr às mudanças) | do fornecedor do SDK |
| Cobertura de UFs/municípios no lançamento | completa (provedor já cobre) | incremental, lenta | completa, mas com defasagem de atualização variável | completa, mas trava a um fornecedor |
| Custo | por documento emitido (variável, previsível) | alto custo fixo de engenharia dedicada | esforço de integração + manutenção de dependência C/Delphi | licenciamento fixo ou por volume, geralmente caro |
| Risco de indisponibilidade do provedor | existe, mitigado por porta + fila | não existe (é nosso) | não existe, mas herda bugs do ACBr | existe, geralmente com SLA contratual |
| Acoplamento à stack Rust | nenhum (é uma API HTTP) | nenhum (nativo) | FFI com C/Delphi — quebra `#![forbid(unsafe_code)]` | depende do SDK (frequentemente C/C++) |
| Tempo até primeira venda com nota emitida | curto (integração de API) | longo (meses de conformidade) | médio (integração de FFI + testes de homologação) | curto, mas com aprisionamento de fornecedor |
| Possibilidade de trocar de fornecedor depois | sim, trivial (troca de adaptador) | não aplicável | difícil (reescrever a camada FFI) | geralmente difícil (contrato e formato proprietários) |

## Decisão

**O Cardeal não implementa emissor fiscal próprio; delega assinatura, transmissão e obrigações
acessórias a uma API fiscal externa, acessada por uma porta única (`PortaFiscal`) com múltiplos
adaptadores possíveis.**

A porta está definida no [doc 10 §2](../10-modulo-fiscal.md) e cobre emissão, consulta, cancelamento,
carta de correção, inutilização, distribuição DFe, manifestação do destinatário e geração de SPED.
O que fica do nosso lado é deliberadamente amplo: o modelo de dados fiscal, a máquina de estados do
documento, a fila durável, a lógica de contingência, a conciliação com o razão e toda a experiência do
usuário ([doc 10 §9](../10-modulo-fiscal.md)) — só a parte que exige acompanhar 27 SEFAZs e milhares
de municípios em tempo real é terceirizada. Essencial à decisão: **a venda nunca espera a SEFAZ**
([doc 10 §3](../10-modulo-fiscal.md)) — o documento fiscal é assíncrono por design, então mesmo uma
falha total do provedor não impede vender, cobrar ou fechar o caixa.

## Consequências

### Positivas

- Cobertura fiscal completa (27 UFs, milhares de municípios) desde o primeiro cliente, sem meses de
  trabalho de conformidade antes de faturar a primeira nota.
- Nenhuma equipe interna precisa acompanhar notas técnicas mensais da SEFAZ — esforço que
  cresceria proporcionalmente ao número de estados atendidos se fosse nosso.
- Trocar de provedor é trocar uma configuração e reiniciar (`[fiscal] provedor = "..."`,
  [doc 10 §8](../10-modulo-fiscal.md)) — a porta protege o resto do sistema dessa decisão comercial.
- O time de engenharia se concentra no que diferencia o produto (o núcleo financeiro, a experiência do
  Pulso, a modularidade) em vez de replicar um produto de terceiros já maduro no mercado.
- Ambiente de teste determinístico (`FiscalSimulado`) autoriza, rejeita com códigos reais e simula
  timeout sob comando — testável sem depender de homologação real da SEFAZ a cada execução de CI.

### Negativas

- Dependência de um terceiro para uma função crítica de conformidade legal: se o provedor tiver um
  bug de cálculo ou uma falha de conformidade, o cliente final sente o efeito mesmo que o bug não seja
  nosso.
- Custo por documento emitido é uma linha de custo variável recorrente, repassada ao cliente final —
  diferente do custo fixo (amortizável) de um emissor próprio.
- Latência de rede e de terceiro: emissão depende de round-trip com o provedor e deste com a SEFAZ —
  mitigado por ser assíncrono, mas ainda existe uma janela em que o documento fica "pendente".
- Risco de indisponibilidade do provedor: se ele cair, a capacidade de **transmitir** documentos para
  a SEFAZ para — mesmo que vender continue normal.
- Menor controle sobre o roadmap de funcionalidades fiscais: novas obrigações acessórias dependem do
  provedor suportá-las antes de o Cardeal poder oferecê-las.

### Mitigações

- A porta `PortaFiscal` com múltiplos adaptadores possíveis ([doc 10 §2](../10-modulo-fiscal.md))
  evita aprisionamento a um único fornecedor — trocar de provedor não exige tocar em nenhum módulo de
  negócio.
- O fiscal é 100% assíncrono: a venda finaliza, o caixa é atualizado e o cupom sai antes de a SEFAZ
  responder ([doc 01 §3](../01-arquitetura-geral.md), [doc 10 §3](../10-modulo-fiscal.md)) — uma falha
  do provedor nunca bloqueia a operação comercial.
- Fila durável com retentativa e contingência formal (offline com `tpEmis=9`, SVC para UF fora do ar,
  fila simples quando só o provedor está indisponível — [doc 10 §4](../10-modulo-fiscal.md)) absorve
  indisponibilidades temporárias sem intervenção manual.
- A arquitetura de porta permite, no limite, trazer a emissão para dentro no futuro (implementar
  `ApiFiscalHttp` substituído por um emissor próprio) sem tocar em nenhum módulo consumidor — a opção
  fica aberta mesmo que hoje não seja a escolha.
- Conciliação diária fiscal × razão ([doc 10 §7](../10-modulo-fiscal.md)) pega divergências entre o
  que foi emitido e o que foi lançado no mesmo dia, reduzindo o risco de um bug do provedor passar
  despercebido por muito tempo.

## Quando revisitar

- Se o custo por documento cobrado pelos provedores disponíveis crescer a ponto de tornar a operação
  de clientes de alto volume (por exemplo, um posto de combustível com milhares de cupons/mês)
  economicamente desvantajosa frente ao custo de manter um emissor próprio.
- Se um provedor específico acumular indisponibilidade acima do SLA contratado de forma recorrente
  (por exemplo, mais de 4 horas de indisponibilidade mensal em dois meses seguidos) e a troca de
  adaptador não resolver porque o problema é sistêmico do setor.
- Se o volume de clientes do Cardeal justificar, do ponto de vista de negócio, negociar condições
  substancialmente melhores como emissor próprio homologado — decisão de negócio, não só técnica.
- Se uma mudança regulatória tornar a certificação de emissor próprio significativamente mais simples
  (por exemplo, um padrão nacional único substituindo os 5.570 padrões de NFS-e municipais).
- Não revisitaríamos por incidentes isolados de indisponibilidade de um provedor — a fila durável e a
  contingência existem exatamente para absorver isso sem impacto na operação do cliente.
