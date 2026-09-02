# 00 — Visão de produto

## 1. O problema

O ERP brasileiro de pequeno e médio porte vive um dilema conhecido:

- **Sistemas genéricos** exigem que o padeiro navegue por telas de indústria química. O usuário se perde, a
  implantação demora meses e o suporte vira treinamento eterno.
- **Sistemas verticais** (só posto, só hotel, só oficina) resolvem bem um nicho, mas o fornecedor mantém
  cinco bases de código e nenhuma evolui.
- **Sistemas em nuvem pura** morrem quando cai a internet — e no Brasil, no interior, em horário de pico de
  varejo, ela cai. Vender é a única coisa que não pode parar.
- **Sistemas pesados** (JVM, .NET com ORM gordo, Electron) exigem máquinas de R$ 4.000 no balcão para
  processar 300 cupons por dia.

E, transversal a todos: **o financeiro é sempre o último a saber**. A venda acontece no PDV, o recebimento é
lançado três dias depois por alguém digitando de novo, e a projeção de caixa é uma planilha paralela que o
dono mantém no celular.

## 2. A resposta do Cardeal

Um **único executável**, **local-first**, **modular em runtime**, com o **razão contábil como barramento
de integração interno**.

```
Vende no PDV  ──┐
Aluga um item ──┤
Abre uma OS   ──┼──►  mesma transação  ──►  RAZÃO (partidas dobradas)  ──►  caixa, DRE, projeção, BI
Fecha diária  ──┤
Abastece      ──┘
```

Não existe rotina de "integração com o financeiro", não existe job noturno de consolidação, não existe
divergência entre o que o PDV vendeu e o que o financeiro viu. É a mesma escrita, na mesma transação ACID.

## 3. Para quem

Perfis de empresa suportados. Cada um é um conjunto curado de módulos — não uma versão diferente do produto.

| Perfil | Módulos ligados por padrão | Particularidades |
|---|---|---|
| **MEI / autônomo** | financeiro (caixa, receber), clientes, vendas simples | Sem estoque, sem contábil, sem centro de custo. Uma tela de "quanto entrou / quanto sai" |
| **Comércio varejista** | + PDV, estoque, compras, fiscal (NFC-e) | Balança, leitor, gaveta, TEF, sangria/suprimento |
| **Posto de combustível** | + combustível, estoque com tanques | Integração com bombas/concentrador, aferição, encerrante, LMC, medição de tanque, ANP |
| **Micro indústria** | + indústria (ficha técnica, ordem de produção), custo médio por lote | Explosão de estrutura, apontamento de produção, rastreabilidade de lote |
| **Hotel / pousada** | + hotelaria (mapa de quartos, diárias, consumo) | Conta do hóspede, folio, tarifário por temporada, check-in/out |
| **Locadora** | + aluguéis | Contrato, período, devolução, avaria, faturamento recorrente |
| **Assistência técnica** | + ordem de serviço, agenda | Laudo, orçamento, aprovação, garantia, peças |
| **Serviços / consultoria** | + agenda, CRM, contratos recorrentes | NFS-e municipal, honorários, recorrência |

Um mesmo cliente pode ligar módulos de perfis diferentes (um posto com loja de conveniência e lava-jato é
`combustível + PDV + estoque + ordem de serviço`).

## 4. Princípios de produto

1. **A tela inicial é o dinheiro.** Não há "dashboard" genérico. Ao abrir o sistema, o usuário vê o
   [Pulso](12-ui-ux.md#o-pulso) — a linha do tempo do caixa, passado e futuro, com o que exige ação hoje.
2. **O usuário não vê o que não usa.** Menu, campos, relatórios e até colunas de grade são derivados do
   conjunto de módulos ativos. Não há campos cinzas "para quando você contratar".
3. **Vender nunca pode parar.** Nenhuma falha de rede, servidor, SEFAZ ou energia pode impedir a próxima venda.
4. **Teclado antes do mouse.** Operador de balcão trabalha com as duas mãos no teclado. Toda ação crítica tem
   tecla; nada essencial exige mouse.
5. **Nada é apagado.** Correção é estorno, não `UPDATE`. Toda alteração tem autor, horário e valor anterior.
6. **Um número, uma verdade.** O saldo de caixa da tela inicial, do relatório e do BI vêm da mesma consulta
   sobre a mesma tabela. Não existe "o relatório X mostra diferente".
7. **Configuração é exceção.** O sistema chega funcionando com plano de contas, formas de pagamento, NCMs e
   perfis prontos. Configurar é ajustar, não construir.

## 5. O que o Cardeal explicitamente NÃO é

Escopo negativo é tão importante quanto escopo positivo. Não fazemos:

- **Emissor fiscal próprio.** Assinatura XML, comunicação com SEFAZ, esquemas, SPED e manifestação são
  delegados a uma **API fiscal externa** ([doc 10](10-modulo-fiscal.md)). Manter emissor fiscal é um produto
  inteiro; não é o nosso.
- **Contabilidade completa.** Entregamos o razão gerencial e a exportação para o contador (ECD/ECF via API
  fiscal). Não fazemos apuração de lucro real, folha, nem escrituração de terceiros.
- **Folha de pagamento / eSocial.** Integramos, não implementamos.
- **Loja virtual.** Expomos API de catálogo e pedido; a vitrine é de terceiros.
- **Multi-idioma / multi-país.** O domínio é o Brasil. Isso é uma escolha de foco, e ela simplifica
  radicalmente o modelo (moeda única, um sistema tributário, um calendário).

## 6. Métricas de sucesso do produto

| Métrica | Meta |
|---|---|
| Tempo do download até a primeira venda | < 15 minutos |
| Tempo de treinamento de um operador de PDV | < 30 minutos |
| RAM do terminal PDV em operação | < 90 MB |
| Tempo entre `F2 (finalizar)` e cupom impresso | < 400 ms (sem depender da SEFAZ) |
| Vendas perdidas por indisponibilidade | 0 |
| Divergência entre caixa físico e sistema | Explicável em 100% dos casos pelo razão |
| Tempo para um dev novo entregar um módulo simples | < 1 semana |
