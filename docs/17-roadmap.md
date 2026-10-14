# 17 — Roadmap

Cada fase termina com algo **usável em produção por um cliente real**. Não construímos seis meses de
alicerce sem ninguém usando.

> **Estado real:** este documento descreve a **intenção e a ordem**; o que já existe, módulo a módulo
> e com testes, está em [19 — Estado e processo](19-estado-e-processo.md). Onde os dois divergirem, o
> 19 vale.

## Objetivo atual (outubro de 2026)

O Cardeal passa a mirar **comércio e serviços de pequeno porte** (a ideia do *lunno*, sem a stack e sem os
recursos de IA dele): quem vende produtos e presta serviços e precisa saber, a cada venda, **quanto
vai lucrar depois de custo, operação e imposto**. Isso se apoia em quatro eixos, nesta ordem de
importância:

1. **Estrutura mercadológica** — árvore Grupo › Subgrupo › Categoria › Subcategoria, com margem
   desejada e perfil fiscal herdados pela árvore e edição em lote ([módulo estoque](modulos/estoque.md)).
2. **Preço por markup com a carga tributária somada** — custo + custo operacional + imposto + comissão
   + taxas + margem desejada, com o imposto vindo do regime e dos campos fiscais do produto
   ([ADR-0018](adr/0018-formacao-de-preco-por-markup-divisor.md)).
3. **Financeiro completo** — DRE, DFC, aging e cobrança, conciliação bancária, plano de contas por
   segmento, score de saúde ([módulo financeiro](modulos/financeiro.md)).
4. **Estoque completo** — análise ABC/giro/ruptura, inventário, transferência, importação de produto
   por XML.

**Como entregamos: de forma gradual.** Uma fatia por vez, cada uma com domínio testado, tela e
documentação, comitada e usável sozinha. Nada espera uma "grande entrega".

### Regra sobre inteligência artificial

**IA é a última coisa que implementamos.** Já deixamos as **bases** para ela — dados estruturados e
auditáveis, o Razão como fonte única, regras explicáveis que *aprendem por correção do usuário*
(casamento de itens de compra, sugestões da conciliação), eventos no outbox —, mas nenhuma
funcionalidade depende de IA para funcionar. Princípios:

- **Tudo funciona sem IA.** Cada sugestão que um dia vier de um modelo já existe hoje como regra
  determinística (preço sugerido, sugestão de conciliação, score de saúde, alertas por limiar).
- **IA só sugere; quem confirma é o usuário.** Nunca grava sozinha em dinheiro, estoque ou fiscal.
- **Entra por trás de uma porta** (trait), com adaptador desligável e custo/privacidade explícitos.
- **Fica na [Fase 7](#fase-7--inteligência-assistida-por-último).** Qualquer item de IA que apareça em
  outra fase foi movido para lá.

## Fase 0 — Alicerce · *entregue (resta o testkit completo e o teste de queda de energia)*

O núcleo, sem nenhum módulo de negócio. Entregável: um binário que sobe, migra, autentica e registra
lançamentos por API.

- [x] Documentação de arquitetura
- [x] `cardeal-kernel` — Dinheiro, Quantidade, Preco, Percentual, Id, Data, Instante, Documento, Erro
- [x] `cardeal-ledger` — plano de contas, lançamento, construtor validado, estornos, consultas de saldo
- [x] `cardeal-storage` — SQLite, escritor único, group commit, migrações, outbox, backup
- [x] `cardeal-modkit` — trait `Modulo`, manifesto, registro, despacho, permissões
- [x] `cardeal-auth` — usuários, papéis, sessões, dispositivos
- [x] `cardeal-protocol` + `cardeal-server` — HTTP/SSE, multi-tenant *(mDNS e multiterminal local: não)*
- [ ] `cardeal-testkit` — motor de teste, relógio controlado, `assert_lancamento!`
- [ ] `xtask` — semear, orçamento, arquitetura, novo-modulo

**Critério de saída:** teste de queda de energia com 500 ciclos passando; orçamento de RAM cumprido.

## Fase 1 — Financeiro + Clientes · MEI utilizável

Primeiro cliente real: um MEI ou prestador de serviços.

- [x] `cardeal-ui` — design system Rubro, grade virtualizada, campos monetários
- [x] `cardeal-desktop` — shell, sidebar retrátil, paleta de comandos
- [x] `mod-clientes` — pessoas, endereços, contatos
- [x] `mod-financeiro` — caixa, contas a receber, contas a pagar, contas bancárias
- [ ] **Pulso** — Rio do Caixa, fôlego, exige ação hoje
- [x] Recorrências e projeção de fluxo
- [ ] Backup automático e restauração guiada

**Critério de saída:** um MEI real usando por 30 dias sem perda de dado e sem suporte diário.

## Fase 2 — Estoque + Vendas + PDV · Comércio utilizável

- [x] `mod-estoque` — produtos, saldos, movimentos, custo médio *(inventário formal: falta)*
- [x] `mod-vendas` — orçamento, pedido, tabela de preço, condição de pagamento
- [x] `mod-pdv` — frente de caixa, sessão, sangria, suprimento, atalhos, impressão
- [ ] **Modo autônomo** — posto avançado, faixas de numeração, reconciliação
- [ ] `cardeal-fiscal` + NFC-e via API, contingência offline
- [ ] Multiterminal com mTLS e pareamento por mDNS

**Critério de saída:** cenário end-to-end "mercado de bairro" verde; um comércio real com 2 PDVs
operando 30 dias, incluindo pelo menos um evento de queda de rede.

## Fase 3 — Compras + Fiscal completo + Conciliação

- [x] `mod-compras` — entrada por XML, rateio de frete, aprendizado de casamento *(cotação e pedido de compra: faltam)*
- [ ] Distribuição DFe e manifestação do destinatário
- [ ] NF-e de saída, cancelamento, carta de correção, inutilização
- [x] Conciliação bancária — OFX e CSV, com saldo banco × sistema e alerta de pendências *(Pix e CNAB retorno: faltam)*
- [ ] Cobrança: boleto e Pix com baixa automática
- [ ] SPED Fiscal e Contribuições via API

## Fase 4 — Analytics (determinístico)

Números que o usuário consegue conferir, sem modelo estatístico por trás.

- [ ] `cardeal-analytics` — projetor, Parquet, DuckDB, esquema estrela *(hoje: consultas diretas sobre o Razão)*
- [x] Curva ABC, giro, cobertura, ruptura e encalhe (aba Análise do estoque)
- [x] Margem prevista e real (pedido, OS), DRE, DFC (direto e indireto), aging, score de saúde
- [x] Rentabilidade por nível da estrutura mercadológica, comissão por estrutura e a apuração/pagamento mensal da comissão do vendedor *(fora: comissão no PDV e ajuste por devolução)*
- [ ] Preços defasados (custo mudou, margem caiu) e reajuste em lote
- [ ] Cartões fixáveis no Pulso e exportações: Excel, Power BI, Parquet

## Fase 5 — Verticais

Em ordem de demanda comercial. Cada uma reusa o núcleo inteiro.

- [x] `mod-os` — ordem de serviço, laudo, garantia (assistência técnica)
- [x] `mod-agenda` — compromissos, recursos, disponibilidade
- [ ] `mod-crm` — funil, oportunidades, histórico
- [ ] `mod-alugueis` — contratos, períodos, faturamento recorrente
- [ ] `mod-combustivel` — bombas, tanques, encerrante, aferição, LMC
- [ ] `mod-hotelaria` — mapa de quartos, folio, diárias, consumo
- [ ] `mod-industria` — ficha técnica, ordem de produção, apontamento
- [ ] `mod-contabil` — mapeamento contábil, ECD/ECF

## Fase 6 — Escala

- [ ] Multi-loja com servidor por filial e consolidação
- [ ] Console web (Leptos) para acesso remoto
- [ ] Aplicativo móvel para vendedor externo e consulta do dono
- [ ] Marketplace de módulos de terceiros
- [ ] Backend PostgreSQL opcional para redes grandes
- [ ] Arquivamento de exercícios

## Fase 7 — Inteligência assistida (por último)

Só começa quando as fases anteriores estiverem em uso real. Tudo aqui **sugere**, nunca grava sozinho,
e o produto continua completo sem nada disto.

- [ ] Copiloto em linguagem natural sobre os dados da empresa
- [ ] Gerador de relatórios por descrição ("clientes que compraram mais de R$ 5.000 e estão inadimplentes")
- [ ] Lançamento por foto/recibo (OCR) e classificação de extrato por modelo
- [ ] Sugestão de NCM/CFOP por descrição do produto
- [ ] Previsão de demanda e de fluxo de caixa; radar de anomalias
- [ ] Padronização de descrição de produto na importação de XML por modelo (o dicionário determinístico
      vem antes, na Fase 3)

**Bases já prontas para isto:** Razão e outbox como fonte única e auditável; regras que aprendem por
correção do usuário; porta fiscal e portas de módulo substituíveis; dados em formato aberto.

## Não faremos (escopo negativo)

Reafirmado de [doc 00 §5](00-visao-produto.md): emissor fiscal próprio, contabilidade completa,
folha de pagamento, loja virtual, internacionalização.

## Como priorizamos

1. **Nada entra sem cliente real esperando.** Funcionalidade especulativa é dívida.
2. **Pilar violado bloqueia release.** Se o orçamento de RAM estourou ou o teste de energia falhou,
   a funcionalidade espera.
3. **Vertical só depois de o núcleo aguentar.** Um módulo de hotelaria sobre um núcleo instável
   multiplica o problema por dois.
4. **Correção de bug de dinheiro passa na frente de tudo.**
5. **IA vem por último.** Nenhuma funcionalidade de IA entra antes de a camada determinística
   equivalente existir, estar testada e estar em uso.
