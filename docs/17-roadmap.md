# 17 — Roadmap

Cada fase termina com algo **usável em produção por um cliente real**. Não construímos seis meses de
alicerce sem ninguém usando.

## Fase 0 — Alicerce · *em andamento*

O núcleo, sem nenhum módulo de negócio. Entregável: um binário que sobe, migra, autentica e registra
lançamentos por API.

- [x] Documentação de arquitetura
- [ ] `cardeal-kernel` — Dinheiro, Quantidade, Preco, Percentual, Id, Data, Instante, Documento, Erro
- [ ] `cardeal-ledger` — plano de contas, lançamento, construtor validado, estornos, consultas de saldo
- [ ] `cardeal-storage` — SQLite, escritor único, group commit, migrações, outbox, backup
- [ ] `cardeal-modkit` — trait `Modulo`, manifesto, registro, despacho, permissões
- [ ] `cardeal-auth` — usuários, papéis, sessões, dispositivos
- [ ] `cardeal-protocol` + `cardeal-server` — HTTP/WS, mDNS
- [ ] `cardeal-testkit` — motor de teste, relógio controlado, `assert_lancamento!`
- [ ] `xtask` — semear, orçamento, arquitetura, novo-modulo

**Critério de saída:** teste de queda de energia com 500 ciclos passando; orçamento de RAM cumprido.

## Fase 1 — Financeiro + Clientes · MEI utilizável

Primeiro cliente real: um MEI ou prestador de serviços.

- [ ] `cardeal-ui` — design system Rubro, grade virtualizada, campos monetários
- [ ] `cardeal-desktop` — shell, sidebar retrátil, paleta de comandos
- [ ] `mod-clientes` — pessoas, endereços, contatos
- [ ] `mod-financeiro` — caixa, contas a receber, contas a pagar, contas bancárias
- [ ] **Pulso** — Rio do Caixa, fôlego, exige ação hoje
- [ ] Recorrências e projeção de fluxo
- [ ] Backup automático e restauração guiada

**Critério de saída:** um MEI real usando por 30 dias sem perda de dado e sem suporte diário.

## Fase 2 — Estoque + Vendas + PDV · Comércio utilizável

- [ ] `mod-estoque` — produtos, saldos, movimentos, custo médio, inventário
- [ ] `mod-vendas` — orçamento, pedido, tabela de preço, condição de pagamento
- [ ] `mod-pdv` — frente de caixa, sessão, sangria, suprimento, atalhos, impressão
- [ ] **Modo autônomo** — posto avançado, faixas de numeração, reconciliação
- [ ] `cardeal-fiscal` + NFC-e via API, contingência offline
- [ ] Multiterminal com mTLS e pareamento por mDNS

**Critério de saída:** cenário end-to-end "mercado de bairro" verde; um comércio real com 2 PDVs
operando 30 dias, incluindo pelo menos um evento de queda de rede.

## Fase 3 — Compras + Fiscal completo + Conciliação

- [ ] `mod-compras` — cotação, pedido, entrada por XML, rateio de frete, aprendizado de casamento
- [ ] Distribuição DFe e manifestação do destinatário
- [ ] NF-e de saída, cancelamento, carta de correção, inutilização
- [ ] Conciliação bancária (OFX, Pix, CNAB retorno)
- [ ] Cobrança: boleto e Pix com baixa automática
- [ ] SPED Fiscal e Contribuições via API

## Fase 4 — Analytics + Inteligência

- [ ] `cardeal-analytics` — projetor, Parquet, DuckDB, esquema estrela
- [ ] Tela de Perguntas e cartões fixáveis no Pulso
- [ ] Curva ABC, margem real, cesta de compras, ruptura e encalhe
- [ ] Previsão de fluxo de caixa e de demanda
- [ ] Radar de anomalias
- [ ] Exportações: Excel, Power BI, Parquet

## Fase 5 — Verticais

Em ordem de demanda comercial. Cada uma reusa o núcleo inteiro.

- [ ] `mod-os` — ordem de serviço, laudo, garantia (assistência técnica)
- [ ] `mod-agenda` — compromissos, recursos, disponibilidade
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
