# 18 — Glossário

Vocabulário do domínio. Se um termo aparece no código, aparece aqui.

## Contábil e financeiro

| Termo | Significado |
|---|---|
| **Partidas dobradas** | Método em que todo lançamento tem débito e crédito de igual valor. A base do razão. |
| **Razão** | O livro de todos os lançamentos. No Cardeal, `cardeal-ledger` e as tabelas `razao_*`. |
| **Lançamento** | Um registro contábil completo: cabeçalho + duas ou mais partidas somando zero. |
| **Partida** | Uma linha do lançamento: conta, valor (positivo = débito, negativo = crédito) e atributos. |
| **Débito** | Aumenta ativo e despesa; diminui passivo, PL e receita. Positivo no nosso modelo. |
| **Crédito** | O inverso. Negativo no nosso modelo. |
| **Conta sintética** | Agrupa outras contas. Não recebe lançamento. |
| **Conta analítica** | Folha da árvore. Recebe lançamento. |
| **Competência** | Data em que o **fato** ocorreu (a venda, o consumo). Regime de competência. |
| **Caixa (regime)** | Data em que o **dinheiro** andou. Regime de caixa. |
| **Liquidação** | O momento em que o dinheiro efetivamente entrou ou saiu. |
| **Título** | Um direito (a receber) ou obrigação (a pagar), com uma ou mais parcelas. |
| **Parcela** | Uma fração do título, com vencimento e valor próprios. |
| **Baixa** | O ato de liquidar (total ou parcialmente) uma parcela. |
| **Estorno** | Lançamento espelhado que anula outro. Nunca apagamos: estornamos. |
| **Contraparte** | Com quem foi a operação: cliente, fornecedor, funcionário, sócio. |
| **Centro de custo** | Divisão interna (loja, setor, veículo) a que a despesa/receita se atribui. |
| **DRE** | Demonstração do Resultado do Exercício: receitas − custos − despesas = resultado. |
| **CMV** | Custo da Mercadoria Vendida. Débito de custo com crédito de estoque, em toda venda. |
| **Fluxo de caixa** | Entradas e saídas de dinheiro no tempo. |
| **Fôlego** | *(termo do Cardeal)* Quantos dias o caixa dura no ritmo atual. |
| **Rio do Caixa** | *(termo do Cardeal)* O gráfico contínuo passado→futuro do Pulso. |
| **Pulso** | *(termo do Cardeal)* A tela inicial financeira. Substitui o "dashboard". |
| **PMR / PMP** | Prazo médio de recebimento / de pagamento. |
| **Ciclo financeiro** | PMR + prazo de estoque − PMP. Quanto tempo o dinheiro fica preso. |
| **Conciliação bancária** | Casar o extrato do banco com os lançamentos do sistema. |
| **Sangria** | Retirada de dinheiro do caixa durante o expediente. |
| **Suprimento** | Colocação de dinheiro no caixa (troco inicial, reforço). |
| **Quebra de caixa** | Diferença entre o valor contado e o valor esperado no fechamento. |
| **Provisão** | Obrigação reconhecida antes do desembolso. |

## Fiscal

| Termo | Significado |
|---|---|
| **NF-e (modelo 55)** | Nota Fiscal eletrônica. Vendas para empresa, transporte de mercadoria. |
| **NFC-e (modelo 65)** | Nota Fiscal de Consumidor eletrônica. Substitui o cupom fiscal no varejo. |
| **NFS-e** | Nota Fiscal de Serviço eletrônica. Municipal — milhares de padrões diferentes. |
| **CT-e / MDF-e** | Conhecimento de Transporte / Manifesto de Documentos Fiscais. |
| **SAT-CF-e** | Equipamento fiscal usado em São Paulo como alternativa à NFC-e. |
| **SEFAZ** | Secretaria da Fazenda estadual. Autoriza os documentos. |
| **Chave de acesso** | Identificador de 44 dígitos de um documento fiscal. |
| **Protocolo de autorização** | Comprovante de que a SEFAZ autorizou. |
| **Contingência** | Modo de emissão quando a SEFAZ ou a internet está indisponível. |
| **Inutilização** | Declarar à SEFAZ que uma faixa de números não foi usada. |
| **Carta de correção (CC-e)** | Corrige erros que não afetam valores nem partes. |
| **Denegação** | SEFAZ recusa por irregularidade cadastral do destinatário. Não pode ser cancelada. |
| **DFe / Distribuição** | Serviço que lista as notas emitidas **contra** o meu CNPJ. |
| **Manifestação do destinatário** | Confirmar, desconhecer ou recusar uma nota recebida. |
| **SPED** | Sistema Público de Escrituração Digital. |
| **EFD Fiscal / Contribuições** | Escrituração fiscal digital de ICMS-IPI / PIS-COFINS. |
| **ECD / ECF** | Escrituração Contábil Digital / Escrituração Contábil Fiscal. |
| **CFOP** | Código Fiscal de Operações e Prestações. Classifica a natureza da operação. |
| **CST / CSOSN** | Código de Situação Tributária (regime normal / Simples Nacional). |
| **NCM** | Nomenclatura Comum do Mercosul. Classifica a mercadoria. |
| **CEST** | Código Especificador da Substituição Tributária. |
| **ICMS-ST** | Substituição tributária: um contribuinte recolhe o imposto de toda a cadeia. |
| **MVA** | Margem de Valor Agregado, usada no cálculo da ST. |
| **DIFAL** | Diferencial de alíquota em vendas interestaduais a consumidor final. |
| **LMC** | Livro de Movimentação de Combustíveis. Obrigatório para postos. |
| **Encerrante** | Contador acumulado da bomba de combustível, em litros. |

## Operacional e varejo

| Termo | Significado |
|---|---|
| **PDV** | Ponto de Venda. A frente de caixa. |
| **Cupom** | O documento da venda no varejo. |
| **Sessão / turno de caixa** | Período entre a abertura e o fechamento de um caixa por um operador. |
| **TEF** | Transferência Eletrônica de Fundos. Integração com a maquininha de cartão. |
| **Adquirente** | Empresa que processa cartões (Cielo, Rede, Stone...). |
| **GTIN / EAN** | Código de barras global do produto. |
| **SKU** | Unidade de manutenção de estoque — um item distinto no catálogo. |
| **Curva ABC** | Classificação por relevância: A = poucos itens, muito valor. |
| **Ruptura** | Produto que deveria ter e não tem. Venda perdida. |
| **Encalhe** | Produto parado, consumindo capital de giro. |
| **Custo médio** | Custo do estoque recalculado a cada entrada, ponderado pela quantidade. |
| **Ponto de pedido** | Saldo em que é preciso comprar para não faltar durante o prazo de entrega. |
| **Ficha técnica** | Estrutura de um produto fabricado: quais insumos e quanto de cada. |
| **Folio** | A conta acumulada de um hóspede durante a estadia. |

## Termos técnicos do projeto

| Termo | Significado |
|---|---|
| **Motor** | O processo servidor (`cardeal-server`). |
| **Posto avançado** | A base local do terminal, usada em modo autônomo. |
| **Modo autônomo** | Terminal operando sem o servidor. |
| **Escritor único** | A thread que detém a única conexão de escrita ao banco. |
| **Group commit** | Agrupar várias operações em um `fsync`. |
| **Outbox** | Tabela de eventos gravada na mesma transação do fato, entregue depois. |
| **Unidade de trabalho** | A transação em curso mais o contexto e os eventos acumulados. |
| **Receituário** | A tabela do doc 05 §5 que define quais lançamentos cada operação gera. |
| **Manifesto** | A declaração de identidade e capacidades de um módulo. |
| **Perfil** | Receita curada de módulos e configurações para um tipo de empresa. |
| **Léxico** | Tradução de termos internos para o vocabulário do perfil ativo. |
| **Porta** | Trait que abstrai uma dependência externa. |
| **Prova do razão** | Verificação de que débitos = créditos e que os saldos derivados batem. |
| **Rubro** | O design system do Cardeal. |
| **Faixa de numeração** | Intervalo de números fiscais reservado a um terminal para uso offline. |
