# 19 — Estado atual e processo de trabalho

> **Leia este documento primeiro** — antes do onboarding (doc 16), antes do roadmap (doc 17).
> Eles descrevem o projeto *terminado* e a *intenção*; este descreve **o que existe agora**,
> **como chegamos até aqui** e **onde continuar**. É o documento que qualquer IA ou pessoa
> nova precisa ler para retomar o trabalho sem repetir decisões já tomadas ou quebrar o que
> já funciona.
>
> Mantenha-o atualizado a cada sessão de trabalho relevante. Se ele mentir sobre o estado
> real do repositório, ele é pior que não existir.

## 0. Objetivo atual e regras de condução (2026-10-12)

**Foco:** comércio e serviços de pequeno porte, a partir da ideia do *lunno* (sem a stack e sem a IA
dele). O centro é saber, a cada venda, quanto sobra depois de custo, operação e imposto:
1. **estrutura mercadológica** em árvore (Grupo › Subgrupo › Categoria › Subcategoria), com margem e
   perfil fiscal herdados e edição em lote;
2. **preço por markup com a carga tributária somada** (ADR-0018);
3. **financeiro completo** (DRE, DFC, aging/cobrança, conciliação, plano de contas, score);
4. **estoque completo** (análise ABC, inventário, transferência, importação por XML).

**Regras de condução:**
- **Gradual.** Uma fatia usável por vez (domínio testado + tela + doc + commit/push). Nada espera uma
  grande entrega.
- **IA é a última coisa.** Já existem as bases (dados estruturados, Razão auditável, regras que aprendem
  por correção do usuário, portas substituíveis), mas **nenhuma funcionalidade depende de IA** e
  nada de IA entra antes de a versão determinística equivalente existir e estar em uso. IA só sugere;
  quem confirma é o usuário. Ver [17 — Roadmap](17-roadmap.md), Fase 7.
- **Ordem do que vem agora** (custo de referência, histórico de compras, preços defasados,
  **rentabilidade por nível da árvore**, **seletor em árvore no cadastro**, **importação da estrutura por
  planilha**, **importação de produto por planilha** e **comissão por estrutura mercadológica** (2026-10-13)
  já estão feitos — a onda 4 fechou). Onda 5: **inventário com contagem cega**, **transferência entre locais**, **produto a partir da
  nota com nome padronizado por dicionário**, **validade e perdas** e **custo-hora do técnico** feitos
  (2026-10-13); do **ST/MVA no custo**, a parte determinística está feita (ICMS-ST, FCP-ST, IPI e desconto do XML
  entram no custo e no título); a estimativa do ST pela MVA quando a nota não traz o valor também (2026-10-28). A **apuração e
  o pagamento da comissão do vendedor** estão feitos (2026-10-14), com o ajuste pela devolução de venda
  (2026-10-27) e a **comissão nas vendas do PDV** (2026-10-28: o cupom ganha o vendedor no `F4`, a venda vira
  "venda comissionável" no `vendas` e o estorno do cupom a tira ou estorna a comissão). O **SPED** (EFD ICMS/IPI)
  é gerado no Cardeal desde 2026-10-28 (`mod_fiscal::efd`, dos XMLs das notas emitidas e das compras que entraram;
  só regime normal; a EFD-Contribuições fica fora — acaba em 2027 com a CBS). A **cotação e o pedido de
  compra** (2026-10-31, a última pendência de compras das ondas passadas): a cotação compara as propostas
  item a item (com os fretes somados) e gera um pedido por fornecedor; o pedido aprovado se liga às notas
  (pelo `xPed` do XML ou pelo único em aberto do fornecedor), a conferência aponta o que diverge (a nota
  que diverge não entra sozinha), a confirmação recebe e o estorno desfaz; o saldo que não vem se encerra;
  o radar de reposição mostra o que está a caminho; o pedido vai ao fornecedor em PDF ou pelo WhatsApp.
  **Onda 8 começou** com a **verificação em duas etapas** (2026-10-31, `cardeal_auth::totp`, doc 08 §2.1.1):
  TOTP com códigos de recuperação no login local e no da conta do servidor (desktop e navegador), o
  segredo selado pelo cofre, o bloqueio progressivo valendo também para o código.
  Onda 6 (Financeiro 2) começou: **conciliação com alerta de pendências e saldo banco × sistema**,
  **lançamento pré-preenchido pelo último da pessoa**, **DFC indireto**, **cheques**, **balancete/diário** e o
  **pacote do contador** (PDF + `.xlsx`) e a **régua de cobrança** (fila do dia com envio de um clique) feitos
  (2026-10-14) — **a onda 6 fechou**; o disparo sem clique espera uma API de WhatsApp (decisão de produto).
  **Onda 7 (fiscal real) começou** sobre o `sefaz-api`, o motor fiscal próprio (ADR-0019): **certificado A1**
  conferido e guardado cifrado no cofre, **dados fiscais do emitente**, **ambiente** e **status da SEFAZ**
  feitos (2026-10-14; homologação SC respondeu 107 com o A1 real). A **NFC-e do PDV** saiu (2026-10-15):
  documento criado na venda sem rede, fila, transmissão em segundo plano (tarefa no despacho), conferência
  pela chave antes de reemitir, área Notas fiscais — e o **DANFCE** (80 mm, QR Code), o **cancelamento** e a
  **inutilização** como pedidos na fila, e o **estorno da venda** no PDV com a NFC-e anulada pela fila.
  O MVP do backlog do `docs/21-licoes-do-gestao-raiz.md` §5 (itens 1–8) e os itens 10 (parcelas e
  estorno da entrada de compra), 11 (XMLs no pacote do contador) e 12 (prazo de cancelamento visível)
  estão feitos, o `sefaz-api` devolve o XML do evento (2026-10-26, com a busca do que falta) e as notas
  de compra chegam pela SEFAZ (DFe, item 9, 2026-10-26); o 13 (Simples, orçamento, score) também.
  A contingência offline da NFC-e também (2026-10-27). A 1ª autorizada no RS espera os dados do usuário. O `sefaz-api` publicado precisa ser atualizado para as mudanças valerem em
  produção, e o DFe ainda não foi conferido contra o Ambiente Nacional (pede o certificado com a
  senha).
- **Pendência de confiança:** as tabelas do Simples e as alíquotas precisam ser conferidas com o
  contador; o ST/IPI **da nota** entra no custo (2026-10-13) e o ST estimado pela MVA (sem o valor na nota)
  entra quando quem confirma decide (2026-10-28) — as MVAs e alíquotas do cadastro precisam do contador.

## 1. O que existe, com precisão (tabela dos crates de 2026-09-05; as sessões seguintes, abaixo, a atualizam)

Todo o **workspace compila** (`cargo check --workspace`) e está formatado
(`cargo fmt --check`) neste exato commit. Não existe estado "quebrado" no HEAD — essa é uma
garantia deliberada deste projeto (ver §4 e §7 sobre por quê isso importa tanto).

### 1.1 Crates com domínio real, implementado e testado

| Crate | O que tem | Testes |
|---|---|---|
| `cardeal-kernel` | `Dinheiro`, `Quantidade`, `Preco`, `Percentual` (aritmética exata, sem ponto flutuante), `Id`/`ChaveIdempotencia`/`Versao` (UUIDv7), `Data`/`Instante`/`Competencia`/`Periodo`/`Fuso`, `Cpf`/`Cnpj`/`Uf`/`InscricaoEstadual` (com dígito verificador real), `Erro`/`Detalhes`/`ErroDominio`, utilidades de texto (busca, similaridade, redação de PII) | **44 unit + 7 doctest** |
| `cardeal-ledger` | O Razão: `Conta`/`CodigoConta`/`PapelConta`, `plano_padrao()`, `Lancamento`/`Partida`/`EstadoLancamento`, `ConstrutorLancamento` → `LancamentoBalanceado` (invariante débito=crédito provado no tipo), `Razao` (registrar/estornar/confirmar/liquidar) e `Contas` — genéricos sobre `PortaRazao`, com **duas** implementações: o double em memória e **`RepositorioRazao`** (adaptador SQLite real sobre a `UnidadeDeTrabalho`). `migracoes::conjunto()` traz as tabelas `razao_*`. **`RepositorioRazao::saldo_realizado`** (novo) soma as partidas `Realizado`+`Estornado` de uma conta — o mínimo de agregação sobre `razao_partida` que existia (ver §1.2); tem de somar os dois estados, não só `Realizado`, senão um estorno debita o dobro do que reverteu (a prova é a própria invariante "original + estorno = zero por conta" do domínio). **Novo**: `Conta::abrir_filha`/`CodigoConta::proximo_filho`/`RepositorioRazao::inserir_conta`/`ultimo_codigo_filho` — abre uma conta analítica em runtime, filha de uma sintética existente (usado por `mod-financeiro::CriarContaBancaria`) | **31 unit + 6 integração (SQLite real) + 2 doctest + 1 propriedade** |
| `cardeal-modkit` | Os tipos declarativos (`Manifesto`/`Submodulo`/`Permissao`/`EntradaMenu`/`ContaPadrao`) com `Manifesto::validar()`, o **`RegistroModulos`** (grafo de `depende_de`, ordem topológica, conjunto efetivo por empresa), **e o despacho**: traits `Modulo`/`Comando`/`Consulta`, `Ctx` de execução, `Registro` (declaração encadeável), `Ambiente` (conjunto efetivo por empresa) e `Despachante` — resolve módulo ativo + `cardeal_auth::autorizar` e roda o manipulador dentro da transação do `Escritor` (comando) ou sobre o `Leitor` (consulta); carga/saída em `postcard`; erro de domínio desfaz o `SAVEPOINT` e sobe intacto | **25 unit (7 de despacho end-to-end, SQLite real) + 1 doctest** |
| `cardeal-storage` | **Escritor único + group commit** (`docs/07`, ADR-0012): `Armazenamento` (abre, migra o núcleo, sobe o escritor e o pool de leitura), `Escritor::executar` (enfileira uma `UnidadeDeTrabalho`, `SAVEPOINT` por tarefa, um `COMMIT`/`fsync` por lote; tarefa que falha ou entra em pânico é isolada), `UnidadeDeTrabalho` (`conexao`, `publicar` → outbox na mesma transação, `auditar` → cadeia BLAKE3, `proximo_numero`/`reservar_faixa`, `travar`), `Leitor` (pool `query_only` sobre o snapshot do WAL), executor de migrações com ordenação topológica e hash imutável, `Segredo<T>`, esquema `nucleo_*` do doc 06 (migração `nucleo` v2 adiciona `nucleo_papel_limite`), `ErroArmazenamento::Dominio(Erro)` (`From<Erro>`) — erro de domínio dentro de uma tarefa desfaz o `SAVEPOINT` e sobe intacto | **9 unit (integração, WAL real)** |
| `cardeal-auth` | `hash_senha`/`verificar_senha` (Argon2id real, parâmetros do doc 08), `PoliticaSenha`, `Bloqueio` (progressivo 5→1min/10→15min), `Escopo` (ABAC — "gerente da filial 2 não vê o caixa da filial 1"), **`Usuario`** (autenticar/trocar/redefinir senha, ativar; erro genérico anti-enumeração), **`Papel`/`PapelDeFabrica`/`PoliticaPapel`** (os 9 papéis de §3.2 como regra declarativa expandida contra o catálogo real — sem depender de `cardeal-modkit`), **`ValorLimite`** (§3.4), **`Sessao`/`AutorizacoesEfetivas`** e a função livre **`autorizar(sessao, permissao, recurso)`** (concede a permissão **e** o escopo abrange o recurso). **`RepositorioAuth` + `consultas`**: adaptador SQLite de `nucleo_usuario`/`nucleo_papel`/`nucleo_papel_permissao`/`nucleo_papel_limite`/`nucleo_usuario_papel` sobre a `UnidadeDeTrabalho`, mesmo padrão do `RepositorioRazao` | **50 unit + 7 integração (SQLite real) + 2 doctest** |
| `mod-financeiro` (parcial) | Domínio puro: `ConstrutorTitulo` → `TituloComParcelas` (rateio que conserva o total ao centavo, `categoria` opcional), `Parcela::situacao_em`/`planejar_baixa`/`reverter_baixa` (juros/multa/desconto **calculados na leitura**; `reverter_baixa` é o inverso exato de `aplicar_baixa`, para o estorno), `SessaoCaixa` (**fechamento cego** com apuração de quebra), `Recorrencia` (agora com `categoria` propagada ao materializar), `CategoriaFinanceira` (novo — rótulo livre de custo/receita, fora do plano de contas, para gráficos por categoria/mês), o `MANIFESTO` completo, o `receituario`. `impl Modulo for ModuloFinanceiro` com migrações v1 (`financeiro_titulo`/`financeiro_parcela`/`financeiro_baixa`), v2 (`financeiro_caixa`/`financeiro_sessao_caixa`/`financeiro_movimento_caixa`) e agora v3 (`financeiro_categoria`/`financeiro_recorrencia` + `financeiro_titulo.categoria`), `RepositorioFinanceiro` (todo o SQL do módulo), os eventos, e **catorze comandos** através do despacho de verdade: título/baixa nas duas espécies (`LancarTituloAReceber`/`LancarTituloAPagar`/`BaixarRecebimento`/`BaixarPagamento`, ambos os `LancarTitulo*` com `categoria` opcional), o ciclo inteiro de caixa (`CadastrarCaixa`/`AbrirCaixa`/`RegistrarSuprimento`/`RegistrarSangria`/`FecharCaixa`), os dois de risco Alto (`EstornarBaixa`, `RenegociarTitulo`), `CriarContaBancaria` — e agora **`CriarCategoria`**/**`CriarRecorrencia`** (grava a regra; nenhum título nasce na hora) com **`materializar_recorrencias_pendentes`** (`pub fn`, não `Comando` — "tarefa agendada, sem permissão de usuário" no próprio spec, idempotente checando `financeiro_titulo` por `origem_id`+`emissao` antes de gerar) pronta para um agendador chamar. Duas consultas novas: `Categorias` (seletor) e `TotalPorCategoriaNoPeriodo` (agregação em memória sobre `financeiro_baixa`, a base para gráficos de churn/receita recorrente por projeto que o usuário pediu) | **45 unit + 19 integração (Despachante + SQLite real) + 1 doctest** |
| `mod-clientes` (parcial) | Domínio puro: `Pessoa`/`Papel`/`ConstrutorPessoa` (papéis que coexistem, FSM `Ativa→Inativa→Anonimizada`, anonimização LGPD que preserva o `id`), `DocumentoPessoa`/`Endereco`/`Contato`, `LimiteCredito`/`Score`, `dedup`, `MANIFESTO`. **E agora no despacho de verdade**: `impl Modulo for ModuloClientes`, migrações (`clientes_pessoa`/`clientes_papel`/`clientes_documento`/`clientes_limite_credito`, e agora v2 `clientes_contato`/`clientes_endereco`), `RepositorioClientes`, os comandos `CriarPessoa`/`EditarPessoa`/`AdicionarPapel`/`DefinirLimiteCredito` — a fatia mínima que `os` (e depois `vendas`/`compras`) precisam — mais **`AdicionarContato`/`AdicionarEndereco`** (novos, auditoria de produção de 2026-09-06: não dava para guardar telefone/e-mail/endereço do cliente, inviável para uso real numa assistência técnica) — e duas consultas: `DetalhePessoa` (ficha completa: documentos + limite de crédito + agora contatos + endereços) e `PessoasPorPapel` (lista/busca por papel, `LIMIT 200` — sem `Pagina`/`Cursor` ainda, ver §1.2). `ConsultarCadastroSefaz`, dedup/mesclagem e LGPD ficam para quando tiverem consumidor | **24 unit + 9 integração (Despachante + SQLite real)** |
| `mod-estoque` (parcial) | Domínio puro: `Produto`/`Variacao`/`validar_gtin`/`Conversao`/`Lote`, `SaldoLocal`/`custo_medio_movel`, `Inventario`, `receituario`, `MANIFESTO`. **E agora no despacho**: `impl Modulo for ModuloEstoque`, migrações (`estoque_grupo_produto`/`estoque_unidade`/`estoque_produto`/`estoque_local`/`estoque_saldo_local`/`estoque_movimento`, e agora v2 adiciona `estoque_produto.codigo_barras` + índice único parcial), `RepositorioEstoque`, e os comandos de cadastro (`CriarGrupoProduto`/`CriarUnidade`/`CriarProduto` — agora aceita `codigo_barras` opcional, validado por `validar_gtin`, que já existia mas estava sem consumidor — `/CriarLocal`) e movimento (`RegistrarEntrada`/`RegistrarSaida`, e agora **`AjustarSaldo`** — correção manual de saldo fora de inventário, o único comando de movimento desta fatia que posta no razão, `receituario::ajuste_manual`), mais a consulta **`ProdutoPorCodigoBarras`** (nova, para bipar no balcão) — sem grade/lote/validade/transferência/o ciclo formal de inventário ainda (`Inventario` é domínio testado mas sem comando — `AjustarSaldo` cobre o caso do dia a dia de um item por vez, ver `docs/modulos/estoque.md` §5). O corpo de `RegistrarEntrada`/`RegistrarSaida` também é função `pub` (`registrar_entrada_comum`/`registrar_saida_comum`) para outro módulo já dependente chamar direto, na mesma transação (`os.AplicarPeca` já faz isso) | **23 unit + 6 integração (Despachante + SQLite real)** |
| `mod-os` (parcial, **novo**) | Construído do zero nesta sessão a partir de `docs/modulos/os.md` — nenhuma inspiração externa foi necessária, o spec já era completo. Domínio puro: `OrdemServico`/`EstadoOs` (FSM completa `Aberta→EmDiagnostico→AguardandoAprovacao→Aprovada→EmExecucao→Concluida→Faturada`, mais `Cancelada`/`Reprovada`), `LaudoTecnico`, `ItemPeca`/`ItemMaoDeObra`, `receituario` (lançamento combinado de faturamento — receita de serviço + custo de peça na mesma partida dobrada; **corrigido** na auditoria de 2026-09-06 para retornar `Option<LancamentoBalanceado>` — uma OS de garantia com `valor_total` zero não gera mais um `ErroRazao::ValorZerado`/pânico, só lança o custo da peça consumida quando houver). `impl Modulo for ModuloOs`, migrações (`os_ordem_servico`/`os_laudo_tecnico`/`os_item_peca`/`os_item_mao_de_obra`, e agora v2 adiciona `os_ordem_servico.itens_orcamento`) e **treze comandos** cobrindo o ciclo inteiro exceto `AcionarGarantia`/reincidência (por isso `coberto_garantia` nunca é `true` nesta versão). `AplicarPeca` chama `mod_estoque::registrar_saida_comum` direto; `FaturarOrdemServico` monta o próprio lançamento combinado e grava o título a receber com `mod_financeiro::ConstrutorTitulo` + `RepositorioFinanceiro::inserir_titulo` (não usa `lancar_titulo_comum` — ver o porquê no doc do próprio comando). **`CancelarOrdemServico`** (revisão de código anterior encontrou que o domínio/permissão/migração já existiam desde o início, mas o comando nunca tinha sido escrito, deixando "cliente desiste antes de aprovar" sem caminho nenhum). **`RemoverItemOrcamento`** (novo, auditoria de 2026-09-06: corrige um item de peça/mão de obra lançado errado sem cancelar a OS inteira; `RegistrarLaudo` também ficou reentrante em `EmDiagnostico`, para corrigir um laudo digitado errado — a consulta `laudo_mais_recente` já mostra sempre a última versão). **Sessão de 2026-09-11 (pedido explícito do usuário — apontamento de tempo é prioridade máxima)**: novo domínio `ApontamentoDeTempo` (`iniciar`/`encerrar`/`ajustar`, migração v3 `os_apontamento_tempo`) — **diferente** de `ItemMaoDeObra.horas` (aquele é a mão de obra orçada/cobrada; isto é o relógio de ponto real, para produtividade e custo real). Três comandos novos (`IniciarApontamento` — recusa um segundo apontamento aberto do mesmo técnico em qualquer OS, `EncerrarApontamento`, `AjustarApontamento` — sempre com motivo, mesma disciplina de `EstornarBaixa`/`RenegociarTitulo`) e cinco consultas novas (`OrdensAguardandoAprovacao`/`HistoricoDoEquipamento` — os dois gaps de consulta que este mesmo documento apontava; `ApontamentosDaOrdem`/`TempoTotalDaOrdem`/`TempoPorTecnicoNoPeriodo`). `mod-financeiro` ganhou `TituloDaOrigem` (consulta pública nova, `financeiro_titulo` por `origem_modulo`/`origem_id`) para a UI de OS mostrar o título gerado ao faturar sem ler a tabela de outro módulo direto. Tela (`cardeal-desktop::tela_os`): seção "Apontamento de tempo" (iniciar/encerrar + tempo acumulado) e "Financeiro" (título gerado) no detalhe da OS, mais uma aba "Lucratividade" nova consumindo `cardeal-analytics` | **22 unit + 3+7 integração (apontamento + ciclo completo, cliente → estoque → OS → financeiro)** |
| `cardeal-analytics` (parcial, **novo**, 2026-09-11) | Era stub puro. Ganhou o primeiro caso de uso real, a pedido do usuário ("onde está entrando dinheiro ou não"): [`margem::MargemDeOs`]/[`margem::MargemAgregada`] — domínio puro de lucratividade de OS, calculado só a partir das consultas públicas de `mod-os` (`BuscarDetalheOrdem`/`ApontamentosDaOrdem`), nunca lendo `os_*` direto. Receita e custo real de peça sempre calculáveis (`ItemPeca::custo_unitario` vem do estoque de verdade); custo/margem de mão de obra fica `Option::None` explícito quando falta custo/hora do técnico cadastrado (`CustoHorario`) — nunca um número inventado. Testado com os três cenários que o usuário pediu: peça com margem boa, OS no prejuízo (peça mal orçada, custo real > cobrado) e OS de garantia (peça a custo zero pro cliente, custo real ainda debitado). Sem UI própria — `cardeal-desktop::tela_os` consome direto | **7 unit** |
| `mod-vendas` (parcial, **novo no despacho**) | Domínio puro: `preco_vigente` sobre `TabelaPreco`/`RegraPreco` (faixa de quantidade + promoção com vigência; a regra mais específica vence, desempatada de forma determinística pelo `id` — `UUIDv7`, então a mais recente vence — quando duas regras cadastradas têm exatamente a mesma especificidade), `Orcamento`/`Pedido`/`ItemVenda` (as duas FSMs, total dos itens, preço congelado, faturar irreversível, desconto acima do teto do papel recusado na hora), `Devolucao` (total/parcial com rateio proporcional sem perder centavo), `Comissao` (base líquida de devolução), o `receituario` (faturamento com receita + desconto + CMV, devolução, comissão), `MANIFESTO` validado (depende de financeiro + clientes + estoque). **E agora no despacho**: `impl Modulo for ModuloVendas`, migrações (`vendas_tabela_preco`/`vendas_regra_preco`/`vendas_pedido`/`vendas_item_pedido` — só os submódulos essenciais `pedido`/`tabela_preco`; orçamento/devolução/comissão/contrato recorrente ficam para depois), `RepositorioVendas`, e sete comandos: `CriarTabelaPreco`/`CriarRegraPreco` e o ciclo de pedido inteiro `CriarPedido`→`AdicionarItemPedido` (resolve o preço vigente contra `mod_estoque::RepositorioEstoque::buscar_produto` para o grupo, congela no item, recusa desconto acima do teto do papel via `ctx.limite("vendas.desconto_maximo")`)→`ConfirmarPedido`→`FaturarPedido`/`CancelarPedido`. `FaturarPedido` segue o desenho de `mod-os::FaturarOrdemServico`: consome estoque item a item (`mod_estoque::registrar_saida_comum`, somando o CMV), monta o lançamento **combinado** (receita+desconto+CMV) e, só a prazo, cria um título a receber de parcela única vinculado a esse lançamento (parcelamento de verdade espera uma `CondicaoPagamento` resolúvel, que ainda não existe) | **28 unit (incl. 1 propriedade) + 3 integração (Despachante + SQLite real)** |
| `mod-pdv` (parcial, **novo**) | Construído do zero nesta sessão a partir de `docs/modulos/pdv.md`, pesquisando `../gestao-raiz` primeiro para desconto/múltiplas formas de pagamento/NFC-e antes de desenhar — conclusão: o spec do Cardeal já era mais rigoroso que a referência nos três eixos (desconto com teto e autorização, NFC-e assíncrona, sessão de caixa formal), então o desenho seguiu o spec do Cardeal, não o gestao-raiz. Domínio puro: `Cupom`/`ItemCupom`/`PagamentoCupom`, FSM `EmAndamento→Finalizado`/`Cancelado`, preço congelado no item, `validar_pagamentos` (soma das formas tem que fechar exatamente com o total — troco fica só na tela, não no backend), o `receituario` (um débito por forma de pagamento + desconto + CMV, mesmo desenho do combinado de `mod-vendas`/`mod-os`), `MANIFESTO` (depende de financeiro + clientes + estoque + vendas — nunca decide preço, lê `TabelaPreco`/`RegraPreco` de lá). **E no despacho**: `impl Modulo for ModuloPdv`, migrações (`pdv_faixa_numeracao`/`pdv_cupom`/`pdv_item_cupom`/`pdv_pagamento_cupom`), `RepositorioPdv`, e seis comandos: `AbrirCupom` (não estava no spec original como comando próprio — ver a nota no arquivo), `AdicionarItem`, `AplicarDescontoItem`, `CancelarItem`, `CancelarCupom` e `FinalizarVenda` (consome estoque de verdade e posta o lançamento combinado). Simplificações documentadas: `FaixaNumeracao` virou contador contínuo por terminal em vez de reserva por lote; `AbrirCaixaPdv`/`FecharCaixaPdv`/`RegistrarSangriaPdv` não existem — o front-end chama os comandos de `financeiro` direto; TEF, balança, diário durável (CRC32) e NFC-e ficam para quando existir hardware/`mod-fiscal` de verdade para mirar (ver `src/lib.rs`) | **12 unit + 3 integração (Despachante + SQLite real)** |
| `cardeal-fiscal` (parcial, **novo**) | Porta [`PortaFiscal`](../crates/cardeal-fiscal/src/porta.rs) (síncrona — `Ctx::porta()` ainda não existe, ver §1.2) com `Nsu`/`ResumoDfe`/`XmlNota`, `ChaveAcesso` (44 dígitos, dígito verificador validado), `interpretar()` — parser real de NFe 4.00 via `quick-xml` (não um double fake), e `FiscalSimulado` — o adaptador de testes/homologação que `docs/10-modulo-fiscal.md` §2 já previa, guarda notas em memória (`Mutex`) e serve tanto a testes de integração quanto ao futuro `ApiFiscalHttp` como referência de contrato | **4 unit** |
| `mod-compras` (parcial, **novo**) | Domínio puro: `NotaEntrada`/`EstadoNotaEntrada` (FSM `AConferir→Conferida→Confirmada`/`Devolvida`), `ItemNotaEntrada`/`EstadoCasamento`, a cascata de casamento (`casar`: regra aprendida → NCM + similaridade de descrição ≥0,82 → nada), `PreferenciasCompras` (a resposta desta fase ao pedido do usuário por "vários ajustes de preferências", inspirada no `gestao-raiz` mas sem copiar a ausência de pedido formal dele). **E no despacho**: `impl Modulo for ModuloCompras`, migrações (`compras_nota_entrada`/`compras_item_nota_entrada`/`compras_regra_casamento`/`compras_preferencias`/`compras_estado_dfe`), `RepositorioCompras`, quatro comandos (`DefinirPreferenciasCompras`/`VincularProdutoManual`/`ConfirmarEntrada`/`LancarNotaManual` — este último para compra sem nota fiscal formal, reaproveitando a mesma cascata de casamento/rateio da importação via `chave_acesso: None`) **e duas funções `pub` que não são `Comando`** — `importar_nota_da_sefaz`/`verificar_notas_na_sefaz` — porque dependem de `PortaFiscal`, que o despacho ainda não injeta (ver `Ctx::de_sessao` em §1.2). É a peça mais importante desta fase: a varredura da distribuição `DFe` desde o último NSU, resolução automática do fornecedor por CNPJ (`mod_clientes`), rateio de frete/seguro/despesas sem perder centavo (`Dinheiro::ratear_por_pesos`) e, quando as preferências permitem e todo item já casou por regra aprendida, confirmação sozinha: `mod_estoque::registrar_entrada_comum` por item e `mod_financeiro::lancar_titulo_comum` para o título a pagar. `Manifesto::depende_de` declara `clientes`/`estoque`/`financeiro` — sem isso o módulo não sobe e a garantia de que essas tabelas existem não vale nada | **9 unit + 3 integração (SEFAZ simulada + SQLite real, ciclo completo)** |
| `mod-agenda` (parcial, **novo**) | Construído do zero nesta sessão a partir de `docs/modulos/agenda.md`, a pedido do usuário de que a agenda ficasse **bem interligada a `clientes` e `financeiro`**. Domínio puro: `Compromisso`/`EstadoCompromisso` (FSM `Agendado→Confirmado→EmAndamento→Concluido`, mais `Cancelado`/`NaoCompareceu`, nunca apagado) e `Compromisso::sobrepoe` (a regra de conflito de horário, meio-aberta nas bordas), `Recurso`/`DisponibilidadeRecurso` (sala/técnico/equipamento/pessoa + janela recorrente; sem regra = sempre disponível), `MANIFESTO`. **E no despacho**: `impl Modulo for ModuloAgenda`, migração v1 (`agenda_recurso`/`agenda_disponibilidade`/`agenda_compromisso`/`agenda_compromisso_recurso`) com **`agenda_compromisso.cliente` como FK de verdade para `clientes_pessoa(id)`** (não um `Id` solto) — por isso `Manifesto::depende_de` declara `clientes`, mesmo padrão de `mod-compras`; a interligação com `financeiro` é o par `origem_modulo`/`origem_id` do compromisso, o mesmo mecanismo de correlação que `financeiro::Titulo` já usa com `os`/`vendas`/`compras` (`agenda` propositalmente não lança dinheiro — quem cobra pela visita é quem chama `CriarCompromisso`). `RepositorioAgenda`, os eventos, e sete comandos: cadastro (`CriarRecurso`/`DefinirDisponibilidade`) e o ciclo do compromisso (`CriarCompromisso` — recusa sobreposição de recurso por padrão e exige confirmação explícita para um horário fora da `DisponibilidadeRecurso`, `ConfirmarCompromisso`/`IniciarCompromisso`/`ConcluirCompromisso`/`CancelarCompromisso` — nunca apaga). `registrar_nao_comparecimento_pendentes` (`pub fn`, não `Comando`, mesmo padrão de `mod_financeiro::materializar_recorrencias_pendentes`) e quatro consultas — `ProximosCompromissos` é literalmente `"agenda.proximos_compromissos.v1"`, o nome que a tela de agenda (`cardeal-desktop`, ainda rodando sobre um modelo em memória) já esperava. Simplificações desta primeira fatia: `CriarCompromisso` sempre recusa conflito (sem a exceção explícita do spec); `Lembrete` sem domínio nem tabela ainda | **12 unit + 6 integração (Despachante + SQLite real, interligado a `clientes`)** |

**Total: 448 testes passando via `cargo test --workspace` (unit + integração SQLite + doctests). Tudo verde.** (Este número já refletia sessões posteriores a 409 antes mesmo da sessão de apontamento de tempo/analytics de 2026-09-11 chegar; ver as notas de sessão em cada linha da tabela para o que mudou desde então — mantido aqui só o total, para não reescrever cada linha retroativamente.)

**Sessão paralela de 2026-09-11 (worktree isolado, `mod-os`/`mod-estoque` + dois dialogs de
`cardeal-desktop`):** três pedidos do usuário atendidos em conjunto. **(A)** Abertura de OS
passou a exigir só `cliente` + `defeito_relatado` (novo campo em `OrdemServico`, migração
`os` v4, aditiva) — `equipamento` virou opcional; `EditarDadosDaOrdem` (novo comando) completa
os dois depois, em qualquer estado não-terminal. `LaudoTecnico.descricao_problema` foi
deliberadamente mantido como está (decisão registrada em `docs/modulos/os.md` §5) para não
sair do escopo desta sessão (o dialog de detalhe de OS pertence à revisão geral de UI/UX,
rodando em paralelo). **(B)** `mod-estoque::Produto` ganhou sete campos técnicos opcionais
(`DetalhesTecnicos`: fabricante, MPN, categoria técnica, especificação, compatibilidade,
garantia do fornecedor, localização física — migração `estoque` v3, aditiva) com
`EditarDetalhesTecnicosProduto` (novo comando) para editar depois da criação. **(C)** Auditoria
de integração fechou uma lacuna real de visibilidade: `mod-estoque::SaldoDisponivelDoProduto`
(nova porta pública) + `mod-os::PecasAguardandoEstoque` (nova consulta) — cruza peça orçada
e ainda não aplicada contra o saldo real do estoque, sem nunca ler tabela de outro módulo
direto. `cargo test --workspace` foi de 450 (linha de base desta rodada de três agentes em
paralelo) para **459** (9 testes novos: 7 em `mod-os`, 2 em `mod-estoque`), zero regressões.
Consumidores fora do escopo direto tiveram que ser ajustados pontualmente para a assinatura
nova de `OrdemServico::abrir`/`CriarProduto` (regra do projeto: nunca deixar
`cargo check --workspace` quebrado) — `mod-orcamentos::ConverterOrcamentoEmOs` e o helper de
teste de `cardeal-analytics::margem`.

**Auditoria de produção (2026-09-06):** com o front-end já em construção contra este backend e
uso real da assistência técnica prestes a começar, veio uma segunda rodada de revisão em
`mod-financeiro`/`mod-clientes`/`mod-estoque`/`mod-os` — desta vez focada não em corretude
abstrata, mas em "dá pra operar isso no mundo real amanhã?". Achados corrigidos (autorizados
pelo usuário, "pode iniciar", em ordem de prioridade): (1) `mod-clientes` não guardava
telefone/e-mail/endereço — `AdicionarContato`/`AdicionarEndereco` novos; (2) uma OS de garantia
(peça de troca sem cobrança) nunca conseguia chegar a `Faturada`, porque o lançamento combinado
rejeitava valor zero — `OrdemServico` ganhou `itens_orcamento: u32` (conta itens
separadamente do total em dinheiro) e `receituario::faturar_ordem_servico` passou a retornar
`Option<LancamentoBalanceado>`; (3) não havia como corrigir um laudo ou um item de orçamento
lançado errado sem cancelar a OS inteira — `RegistrarLaudo` ficou reentrante e
`RemoverItemOrcamento` é novo; (4) `mod-estoque` não tinha campo de código de barras —
`CriarProduto` ganhou `codigo_barras` opcional (valida GTIN) e a consulta
`ProdutoPorCodigoBarras`; (5) não havia como corrigir um erro de digitação de saldo em
`mod-estoque` sem editar o SQLite na mão — **`AjustarSaldo`** (novo) fecha essa lacuna,
reaproveitando `SaldoLocal::ajustar` (já existia) e um novo `receituario::ajuste_manual`
(mesma contabilização de `ajuste_de_inventario`) para postar a correção no razão; o ciclo
formal de inventário com contagem cega/múltiplos itens (`CriarInventario` etc.) continua sem
comando — o domínio já existe e é testado, mas é uma peça maior deixada para quando fizer
falta (ver `docs/modulos/estoque.md` §5). Lacunas identificadas e deliberadamente **não**
corrigidas nesta rodada, por serem de
menor risco a curto prazo num deploy de uma empresa só: nenhuma busca por id em
`mod-financeiro`/`mod-clientes`/`mod-estoque`/`mod-os` filtra por `empresa` além do `id`
(inofensivo hoje — `Id` é UUIDv7 de 128 bits, não adivinhável — mas um buraco antes de colocar
uma segunda empresa no mesmo banco), faltam algumas consultas de conveniência
(`SessoesEmAberto`, `HistoricoDoEquipamento`, estoque baixo, busca incremental) e algumas
mensagens de erro técnicas demais para usuário final (`ParcelaNaoBaixavel`, `EstadoInvalido`).

**Extensão em `mod-financeiro` nesta sessão:** `Titulo`/`ConstrutorTitulo` já tinham
`origem_modulo`/`origem_id` prontos (não precisou de migração nova); só faltava
`lancar_titulo_comum` virar `pub` e `DadosLancamentoTitulo` ganhar os dois campos — feito,
e reexportado, para `os` (e depois `vendas`/`compras`) chamarem direto quando o caso for o
simples (um único lançamento = o título). Quando o caso exige lançamento **combinado**
(receita + CMV juntos, como em `os.FaturarOrdemServico`), o padrão é outro: montar o
lançamento com `ConstrutorLancamento` direto e gravar o título com `ConstrutorTitulo` +
`RepositorioFinanceiro::inserir_titulo`, vinculando `parcela.lancamento` ao lançamento já
registrado — nunca chamar `lancar_titulo_comum` nesse caso, ele criaria um segundo
lançamento e duplicaria a receita.

### 1.2 Crates ainda como stub

Os outros 13 crates do workspace (`cardeal-protocol`, `cardeal-server`, `cardeal-cliente`,
`cardeal-ui`, `cardeal-testkit`, `cardeal-desktop`,
`xtask`, `mod-crm`, `mod-alugueis`,
`mod-combustivel`, `mod-hotelaria`, `mod-industria`, `mod-contabil`) têm só
`Cargo.toml` válido + `src/lib.rs`/`src/main.rs` mínimo apontando para este documento e
para o roadmap. Isso é proposital: garante que `cargo check --workspace` sempre passa,
mesmo com a maior parte do sistema ainda não escrita — ver §4. (`cardeal-analytics` saiu
desta lista em 2026-09-11 — ver §1.1: ganhou o primeiro domínio real, margem de OS.)

`cardeal-storage`, `cardeal-ledger` (com `RepositorioRazao`), `cardeal-auth` (domínio de
autorização + `RepositorioAuth`) e `cardeal-modkit` (`RegistroModulos` **+ o despacho
`Modulo`/`Comando`/`Consulta`/`Ctx`/`Registro`/`Despachante`**) já existem e passam.

**`mod-financeiro`, `mod-clientes`, `mod-estoque`, `mod-os`, `mod-compras`, `mod-vendas`,
`mod-pdv` e agora `mod-agenda` atravessam o despacho de verdade** — cada um com `impl Modulo`, migrações
próprias, `Repositorio*` e comandos reais testados contra SQLite (detalhe de cada um em
§1.1). `mod-os` foi construído numa sessão inteira, do domínio ao teste de integração, e
prova o ciclo ponta a ponta que a assistência técnica que motivou esta fase realmente
precisa: abrir OS → laudo → orçamento → aprovação → aplicar peça (consumindo estoque de
verdade, `mod_estoque::registrar_saida_comum` chamado direto) → faturar (lançamento
combinado + título a receber real no financeiro). `mod-compras` foi a fase seguinte: a
importação automática de nota de compra via SEFAZ está implementada e provada de ponta a
ponta em `tests/importacao.rs` (`FiscalSimulado` no lugar da SEFAZ real), mais
`LancarNotaManual` para compra sem nota fiscal formal. `mod-vendas` reusou exatamente o
mesmo desenho de `mod-os` para o lançamento combinado de faturamento (receita+desconto+CMV),
com uma diferença: só cria título quando a venda é a prazo — à vista o pagamento já é o
próprio lançamento em Caixa, sem título nenhum. `mod-pdv` fechou o núcleo comercial: a venda
de balcão rápida, reusando `TabelaPreco`/`RegraPreco` de `mod-vendas` (nunca decide preço por
conta própria) e generalizando o lançamento combinado para N formas de pagamento — a mesma
disciplina de "um só lançamento, nunca dois" que `os`/`vendas` já seguiam. `mod-agenda`,
nesta mesma sessão, entrou por um pedido diferente: não completar o núcleo comercial, mas
garantir que compromissos/recursos nascessem **interligados** a `clientes` (FK de verdade
para `clientes_pessoa`, não um `Id` solto) e a `financeiro` (via `origem_modulo`/`origem_id`,
o mesmo padrão de correlação que `Titulo` já usa) — junto com `mod-financeiro` ganhando
recorrência e categoria de verdade (`CriarRecorrencia`/`materializar_recorrencias_pendentes`/
`CriarCategoria`), a pedido do usuário por suporte a custo/receita recorrente (aluguel,
SaaS vendido por assinatura) e gráficos de churn/receita por projeto.

**Extensão em `cardeal-modkit` para viabilizar isto**: `Ctx::de_sessao(sessao, ambiente)`
(novo, `pub`) — as duas funções de importação de `mod-compras` não são `Comando` (dependem
de `PortaFiscal`, que o despacho ainda não injeta) e por isso são chamadas fora do lookup por
nome do `Despachante`; antes disso não havia como montar um `Ctx` de verdade fora dali. É a
mesma lógica de `montar_ctx` que o `Despachante::executar_comando` já usava, só que exposta
para quem tiver uma `Sessao`+`Ambiente` em mãos — hoje o teste de integração, no futuro uma
tarefa agendada. Continua exigindo um `Escritor::executar` em volta para a
`UnidadeDeTrabalho` — não é atalho para fora da transação. Ver `docs/contratos-internos.md`
§7 regra 2.

O que falta, na ordem do roadmap (ver §4): **(a)** `mod-fiscal` + `ApiFiscalHttp` de verdade
(certificado digital, `PortaFiscal` mirada no contrato real do `sefaz-api` — já estudado,
notas em `docs/modulos/financeiro.md` §5; hoje só `FiscalSimulado`); **(b)** ~~`mod-pdv`~~
**feito** — venda de balcão com múltiplas formas de pagamento, sem TEF/balança/NFC-e (ver
§1.1); **(c)** ~~`mod-vendas` no despacho~~ **feito** — pedido inteiro (criar → item → confirmar →
faturar/cancelar) mais tabela/regra de preço, ver §1.1; **(d)** ~~`EstornarBaixa`/
`RenegociarTitulo` no financeiro~~ **feito** (pedido explícito do usuário — "o mais
importante é o financeiro" — ver §1.1); **(e)** a montagem de `AutorizacoesEfetivas` cruzando
os papéis do usuário com o `ConjuntoEfetivo` (em `cardeal-server`); **(f)** `Ctx::contas()`
(espera um resolvedor não-genérico em `cardeal-ledger`) — por ora o comando faz
`Contas::nova(&RepositorioRazao::novo(uow), empresa)`. UI (`cardeal-ui`/`cardeal-desktop`)
fica deliberadamente para depois — decisão do usuário, não lacuna esquecida: verificação
até lá é por teste de integração, como já vem sendo feito.
Ver §4.

### 1.3 Documentação

Completa: 19 docs de arquitetura (00–18, mais este), 14 ADRs (`docs/adr/`), specs funcionais
de 9 dos 14 módulos (`docs/modulos/` — faltam `alugueis`, `combustivel`, `hotelaria`,
`industria`, `contabil`), e o contrato de assinaturas entre crates
(`docs/contratos-internos.md`). Tudo escrito **antes** do código correspondente — é a ordem
que este projeto segue, não um acidente (ver §4).

### 1.4 Git

Repositório local com **17 commits**. Remoto: `https://github.com/IgorGewehr/cardeal.git`.
O primeiro push foi feito; `origin/master` está em `11a8860` (`mod-vendas`) e o local está
**à frente** dos commits de `cardeal-storage`, `cardeal-ledger`/`RepositorioRazao` e
`cardeal-modkit`/`RegistroModulos` (mais o de `cardeal-auth` desta sessão, se já commitado).
Quem retomar deve rodar, no terminal do usuário, e confirmar com `git log origin/master`:

```
git push origin master
```

### 1.5 Sessão 2026-09-25 — OS e financeiro no dia a dia

A partir de um levantamento de UI/UX + backend pedido pelo usuário ("mais ágil, elegante,
eficiente"), corrigido em sequência na branch `refactor/ui-listagem-padrao-os-aplicar-peca`,
um commit por incremento:

- **Furos que afetavam dado real:** recorrência nunca virava título (a materialização não era
  chamada; agora é o comando `financeiro.materializar_recorrencias.v1`, disparado ao entrar e
  ao abrir o Financeiro, recuperando até 60 dias de ocorrências perdidas); baixa ignorava a
  conta bancária; diálogo de baixa guardava a posição da parcela no vetor; KPIs transformavam
  erro em R$ 0,00; OS só faturava à vista.
- **Atomicidade:** `os.abrir_ordem_com_cliente_novo.v1` (cliente + OS num COMMIT, via
  `criar_pessoa_comum`/`adicionar_contato_comum` de `mod-clientes`), `os.aplicar_pecas.v1` e
  `financeiro.baixar_{recebimentos,pagamentos}_em_lote.v1` são tudo-ou-nada.
- **Busca e desempenho:** `os.buscar_ordens.v1` filtra no motor (acabou o teto de 500 que
  escondia OS antigas); cada ação na OS recarrega só a lista (não todos os catálogos); o
  extrato rotula OS com `os.ordens_por_id.v1` (antes um detalhe completo por linha).
- **Recursos:** ficha de entrada da OS (previsão de entrega, nº de série, acessórios —
  migração os v6) com atrasadas na lista; linha do tempo (`os_historico`, migração os v7,
  gravada pelo `RepositorioOs`); detalhe da OS em `Gaveta` com um único "próximo passo";
  histórico do aparelho; preço sugerido da peça (tabela → `os.ultimo_preco_da_peca.v1`);
  WhatsApp com mensagem por estado; `financeiro.situacao_da_parcela.v1` (a baixa sugere o
  total com juros/multa); renegociação pela tela.
- **Estrutura:** `tela_os` e `tela_financeiro` viraram diretórios de módulos (como
  `tela_pdv`); `SeletorPagamento`, `Gaveta` e `LinhaDoTempo` novos em `cardeal-ui`; modo de
  captura do app (`--features demo`, `CARDEAL_DEMO`).

Deixado de fora de propósito: desconto no faturamento da OS (precisa de coluna nova e mexe na
margem do `cardeal-analytics`) e entrada + restante (o `ConstrutorTitulo` rateia por igual;
exige parcela de valor diferente). Testes: 575 → 605.

### 1.6 Sessão 2026-09-29 — Financeiro autônomo

Pedido: operar o dia a dia só pela tela do Financeiro (cadastrar cliente, lançar pagamento),
mostrando de onde vem cada valor.

- **Lançamento já quitado:** `LancarTituloA{Receber,Pagar}.quitado_agora: Option<QuitadoAgora>`
  (`comandos/quitado_agora.rs`) grava título + baixa no mesmo COMMIT; exige a permissão de
  baixa da espécie e parcela única. O diálogo de lançamento usa o `EstadoPagamento`
  (à vista = quitado agora; a prazo = parcelas), tem "Lançar e continuar" e abre a receber
  **ou** a pagar de qualquer aba (`FormLancar.a_receber`).
- **Pessoa na hora:** `crate::pessoa::EstadoPessoa` (desktop) é o bloco "existente (busca por
  nome/telefone/documento) ou novo (nome, telefone, documento, aviso de duplicado)" — usado
  pela nova OS, pelo lançamento e pelo cadastro rápido (botão "+ Cliente"/"+ Fornecedor" no
  cabeçalho). Não copie esse bloco: use o módulo.
- **Origem:** coluna Origem nas abas A receber/A pagar (`OS #12 — Maria`, Peça de OS, Compra,
  PDV, Venda, Recorrência, Manual); a busca casa por descrição e origem. Sem permissão de OS o
  rótulo fica "OS", sem erro.
- **Margem na baixa:** a baixa de uma parcela vinda de OS mostra total, custo das peças e
  margem (R$ e %) pela consulta nova `os.margem_da_ordem.v1`; peça ainda não aplicada é
  avisada, não estimada. As consultas de margem do mod-os foram para `consultas/margem.rs`
  (`consultas.rs` virou diretório — estava em 835 linhas).
- Cena de captura nova: `financeiro-lancar`; `financeiro-baixa` abre a parcela da OS.

Os testes `criar_recorrencia_*` dependiam da data ("dia 1" + 35 dias de antecedência pegava
duas ocorrências no fim do mês); agora usam o dia de hoje + 20 dias.
`comandos/conta_a_pagar_avulsa.rs` (trabalho anterior, ainda sem chamador) foi commitado em 2026-10-01; falta ligar `mod-os`/`mod-compras` a ele.

### 1.7 Sessão 2026-09-30 — Custos por categoria e finanças pessoais

- **A pagar não é só fornecedor:** a contraparte vira "Favorecido" (sócio, locador,
  concessionária), a categoria fica ao lado da descrição no lançamento e ganha coluna e filtro
  (`FiltroCategoria`) na lista. `financeiro.criar_categorias_sugeridas.v1` cria as que faltam
  (Pró-labore, Aluguel, Energia, Contador, Peças…; `CATEGORIAS_SUGERIDAS`).
- **Aba "Custos por categoria"** (`tela_financeiro/custos.rs`): consulta
  `financeiro.projecao_por_categoria.v1` (`src/projecao.rs`) — por categoria e mês, o realizado
  (baixas), o em aberto (o vencido conta no mês corrente) e as ocorrências de recorrência que
  ainda não viraram título (depois de hoje + antecedência). Clique na categoria abre A pagar
  filtrado por ela.
- **Chave Empresa | Pessoal:** `mod_financeiro::pessoal` (tabela `financeiro_pessoal`,
  migração financeiro v5, permissão `financeiro.pessoal`) — livro previsto × pago **fora do
  Razão**, privado por `usuario` (todo SQL filtra por ele). Repetição única, parcelada (total
  rateado) ou mensal (valor cheio); cartão por lançamento → faturas por mês. Tela em
  `tela_financeiro/pessoal/` (visão com projeção 12 meses e saldo acumulado, lançamentos do
  mês, cartões com "marcar fatura paga").
- Cenas novas: `financeiro-custos`, `financeiro-pagar`, `financeiro-pessoal`,
  `financeiro-cartoes`, `financeiro-pessoal-novo`.

### 1.8 Sessão 2026-10-01 — Lançar sem pessoa e acabamento do Financeiro

- **Sem cliente/favorecido:** `crate::pessoa::EstadoPessoa` ganhou `nenhuma` (só quando o bloco
  é `opcional`): a chave vira "Sem favorecido | Favorecido existente | Novo favorecido". O
  lançamento do Financeiro começa em "Sem…" (`EstadoPessoa::sem_pessoa()`) e o bloco foi para
  depois de valor/data; `lancar` usa `escolhida()`/`cadastrando()`, que respeitam o "Sem"
  mesmo com algo digitado antes.
- **Acabamento:** toda escolha segmentada da tela (Empresa/Pessoal, período, 30/90 dias,
  "Serve para", meses da análise, existente/novo) agora é `Abas` — antes eram laços de
  `Botao::primario/fantasma` com cara de botão vermelho solto. Cabeçalho com hierarquia
  (lançamentos em primário/secundário, cadastros em fantasma). `BarraFiltros::acao` (novo,
  com entrada na galeria) põe "Receber/Pagar várias" na linha dos filtros; a faixa de
  seleção virou `Painel` realçado. Células vazias mostram "—" esmaecido; Fluxo e Bancos
  usam `EstadoVazio`, o total disponível vai no cabeçalho do `Painel` da grade, e
  "Ver tabela" das categorias fica no cabeçalho do cartão.
- Cenas novas: `financeiro-fluxo`, `financeiro-bancos`.
- O teste `baixa_de_parcela_vencida_sugere_o_total_com_multa` dependia do dia do mês
  (vencimento de 10 dias atrás fora do período "Mês" nos dias 1–10); agora fixa o período.
- No banco local do usuário, o movimento financeiro de teste (2 títulos das OS #4/#5, baixas e
  4 lançamentos do Razão) foi apagado à mão, com backup em `~/.local/share/cardeal-backups/`.

### 1.9 Sessão 2026-10-01 (tarde) — Tipografia de verdade e "já pago"

- **Fontes embutidas:** Inter (Regular/Medium/SemiBold) e JetBrains Mono (Regular/Medium) em
  `crates/cardeal-ui/assets/fontes/` via `include_bytes!` — antes só carregavam de
  `C:\Windows\Fonts`, e no Linux tudo saía num peso só. `Papel` ganhou `Acao` (botões),
  `Sobrelinha` (caixa alta espaçada) e `Apoio` (explicação); `Rotulo::sobrelinha`/`apoio`.
  Escala e regra em `docs/12-ui-ux.md` §3. Nas telas, rótulos em CAIXA ALTA viraram
  `sobrelinha` e frases longas em `Rotulo::campo` viraram `apoio`.
- **À vista sem pagar:** `CondicaoPagamento.pago` + `SeletorPagamento::com_pendente("Já pago")`
  (na galeria): desmarcado, o lançamento do Financeiro é parcela única em aberto vencendo na
  data — sem montar "a prazo". `EstadoPagamento::mostrar_com` repassa o rótulo.
- Chave do topo: "Pessoais" (sem o parêntese).

### 1.10 Sessão 2026-10-01 (fim) — Cor como feedback

- `Botao::tom(Tom)`: primário preenchido no tom; secundário/fantasma com texto e contorno no
  tom. Sem tom, o hover de secundário/fantasma acende na marca (contorno, texto e fundo
  `rubro_ativo`); o primário ganha brilho da própria cor no hover; o atalho (`Ctrl+N`) sai
  menor, em mono e apagado. Galeria tem os quatro casos.
- `Abas`: a ativa em `rubro`, a inativa com fundo `superficie_hover` no hover.
- Financeiro: verde = dinheiro que entra, vermelho = que sai — "+ A receber"/"+ A pagar",
  "Lançar…", "Confirmar recebimento/pagamento", "Baixar selecionadas". As últimas escolhas
  segmentadas feitas com botões (custos, recorrências, cadastro rápido) viraram `Abas`.
  Nome da contraparte em `texto_forte` na grade.

### 1.11 Sessão 2026-10-01 (noite) — Cardeal online: servidor multi-tenant (ADR-0016)

Branch `feat/servidor-multi-tenant`. Decisão do usuário: web = **egui compilado para WASM**
(paridade total com o desktop), deploy = Docker + Cloudflare Tunnel, RAM e velocidade acima de
tudo. Feito, com números medidos (`crates/cardeal-server/examples/carga.rs`):

- `cardeal-motor` (extraído de `cardeal-cliente`, que agora só reexporta + autoatualizador):
  o motor de **uma** empresa. `Plano` = despacho + conjunto efetivo + migrações preparados uma
  vez por processo e compartilhados. `executar_bruto`/`consultar_bruto` (bytes postcard, o
  caminho da rede), `sessao_do_usuario` (sessão sem senha, para a conta do servidor),
  `definir_empresa_nova`, `podar_idempotencia`. `configurar_inicial` devolve o id do admin.
- `cardeal-distribuicao`: a lista única de módulos (desktop e servidor).
- **Idempotência de verdade**: `Despachante::executar_comando_idempotente` grava a resposta em
  `nucleo_idempotencia` na mesma transação; reenvio devolve a resposta original.
- `ConfigArmazenamento::servidor` (1 leitor, cache 2 MiB/1 MiB, mmap 64 MiB).
- `cardeal-protocol` real (rotas, cabeçalhos, login, `status_http`), sem tokio/rusqlite.
- `cardeal-server`: diretório global (`diretorio.db`: contas e-mail+Argon2id, empresas,
  vínculos conta→empresa→usuário, sessões por BLAKE3 do token), `Frota` (abre sob demanda,
  despeja ociosas, teto, nunca abre a mesma base duas vezes), cache de sessões, limite de login
  por IP, cabeçalhos de segurança, SIGTERM limpo, alocador mimalloc ajustado
  (`src/alocador.rs`, único `unsafe`). 20 testes (11 ponta a ponta por HTTP).
- Imagem Docker de 31 MB (`packaging/servidor/`), container em repouso ~2 MiB. Guia em
  `docs/build/servidor.md`.

Medido: processo vazio 6,9 MB; ~0,8 MB de heap por empresa aberta; consulta quente p50 120 µs
/ p99 210 µs; empresa fria p50 11 ms. Descartado com medição: desligar o lookaside do SQLite
(−90 KB/empresa, mas p50 118→151 µs).

**Depois, na mesma noite (todas as fatias com commit + push na branch):**

- **Domínio compila para WASM.** Feature `sqlite` (padrão) em ledger, auth, modkit e nos
  módulos: repositório/migrações/`fn executar` atrás dela; `Comando`/`Consulta` (tipo da
  saída, `PERMISSAO`) existem sempre (`cardeal-modkit/src/contrato.rs`). Catraca
  `cargo xtask verificar-wasm` (18 crates, `-D warnings`) na CI. Dependências mortas
  removidas (auth: rcgen/subtle/zeroize/blake3; modkit: ahash/indexmap).
- **`mod-empresa`**: dados, identidade visual, logo, usuários e papéis pelo despacho; os
  métodos do `MotorLocal` viraram atalhos sobre o mesmo SQL.
- **`cardeal-cliente`**: fachada `Motor`/`Sessao` (Local | Remoto) — o desktop inteiro fala só
  com ela; `remoto::protocolo` sem I/O; `Remoto` síncrono com reenvio idempotente (mesma
  chave). Desktop ganhou "Entrar no servidor" (`login.rs`) e login rápido (só a área
  inicial carrega).
- **`cardeal-web`** (egui/WASM, glow): login, recarregar sem senha, empresa, lista de
  clientes, sair; `cargo xtask construir-web`; servido pelo próprio servidor (`--web`), cache
  imutável + CSP estrita; imagem Docker com tudo (36 MB).
- **Fontes recortadas** (`scripts/recortar-fontes.sh`): 1,75 MB → 256 KB (desktop e web).
- **Servidor**: impressão do plano (`nucleo_esquema`) — abertura a frio 10,8 → 2,0 ms;
  backup incremental (`VACUUM INTO` + zstd + retenção, sidecar rclone cifrado para o R2);
  métricas Prometheus (`/metricas`); limite de login por IP; SIGTERM limpo.

**Fechamento (2026-10-02):** troca de senha da conta (derruba as outras sessões); o
administrador põe/tira gente da empresa com papel de fábrica (`mod-empresa` +
`/v1/e/{empresa}/membros`); no navegador, barra lateral + Equipe + Minha conta; respostas da
API comprimidas; `cardeal-server saude` + `HEALTHCHECK`; imagem publicada no GHCR pela CI
(`.github/workflows/servidor.yml`); versão do wasm-bindgen sempre a do `Cargo.lock`.
674 testes. **As telas de negócio no navegador ficaram para outro momento** (decisão do
usuário em 2026-10-02).

**Disponibilidade, listas e tempo real (2026-10-02, branch `feat/disponibilidade-tempo-real`):**
replicação contínua do WAL (perda máxima = último commit; `cardeal-server restaurar --de`);
nenhuma lista cortada em silêncio (`TETO_LISTA_COMPLETA`) e `.v2` paginadas por cursor em
clientes, estoque, OS e financeiro; tempo real por SSE a partir do outbox (adendos na
ADR-0016). Desktop remoto (`Remoto::acompanhar`) e navegador (`EventSource`) já reagem. 698
testes. **Ainda não:** as telas do desktop migrarem para as `.v2` (aí as `.v1` saem).

**Organização com vários CNPJs (2026-10-02, ADR-0017):** uma base = uma organização; o
motor faz sessão por CNPJ, `adicionar_empresa` (plano de contas e admin próprios); o
servidor mapeia empresa → base no diretório (v2) e tem `POST /v1/e/{empresa}/empresas`;
sair de um CNPJ não tira o usuário dos outros. Também: reenvio idempotente no navegador e
alerta de falha da réplica (`/saude` → `degradado`). Clientes/fornecedores e o catálogo de
produtos são do grupo (`Ctx::organizacao`); estoque, financeiro, fiscal e OS, de cada CNPJ.
**Pendente:** as telas para criar e trocar de CNPJ (desktop e web), transferência entre CNPJs
e relatórios consolidados.

**Para decidir com o usuário:** o plano das telas assíncronas em `docs/20-cliente-web.md` §3
(muda as telas do desktop, em uso real). **Otimizações medidas em espera:** escritor sem
thread dedicada por empresa; fontes de reserva do egui no web (§4 do doc 20).

### 1.12 Sessão 2026-10-09 — Hierarquia de produtos, preço por markup e DRE

Origem: o spec do **lunno** (outro produto, mesma ideia de ERP para comércio/serviços) foi lido
junto com o Cardeal; o que ele descreve de estoque, markup e financeiro e que o Cardeal não tinha
virou esta sessão. **Fora do escopo de propósito:** tudo que o spec chama de IA (copiloto, OCR por
WhatsApp, classificação de extrato, NCM por NLP), microsserviços/AWS/Postgres e Open Banking.

- **Estrutura mercadológica de 4 níveis** (`mod-estoque::hierarquia`): recusa o 5º nível, ciclo e
  movimento que estoura o teto contando a altura da subárvore. Comandos `RenomearGrupoProduto`,
  `MoverGrupoProduto`, `MoverProdutosDeGrupo`; consulta `ArvoreGruposProduto`.
- **Preço por markup divisor** (`mod-estoque::precificacao`, ADR-0018): margem herdada
  produto → grupo → padrão da empresa; imposto pelo Simples (Anexos I–V, RBT12 do Razão, ST e
  monofásico no Anexo I), MEI sem percentual, Presumido/Real informado; custo operacional
  informado ou derivado do Razão. Migração `estoque` v6 (`estoque_parametros_preco`, margem e perfil
  fiscal no grupo, margem no produto). Consultas `PrecoSugeridoDoProduto` (aceita custo informado),
  `PrecosSugeridosDoGrupo`, `ParametrosDePreco`.
- **DRE gerencial** (`mod-financeiro::dre`): lida do Razão por `cardeal_ledger::resultado_por_conta`,
  comparativo mês × anterior × ano anterior. Propriedade testada: lucro líquido = receitas − despesas.
- **UI:** componente novo `ArvoreHierarquia` (+ galeria); `tela_estoque` virou diretório com as abas
  **Estrutura** e **Preços**; `tela_financeiro` ganhou a aba **Resultado (DRE)**. Cenas de captura
  `estoque-estrutura`, `estoque-precos`, `financeiro-dre`.
- **DFC pelo método direto** (`mod-financeiro::dfc`, `cardeal_ledger::movimentos_de_caixa`): na mesma
  aba do DRE ("Resultado e caixa", chave DRE/DFC); o saldo final bate com o Razão. O método
  indireto não foi feito.
- **Alteração em lote** (`mod-estoque::alteracao_em_lote`, migração `estoque` v7): margem, grupo e
  situação de vários produtos, com **prévia** do antes/depois, histórico e **desfazer a última em 24h**
  (só restaura quem não foi editado depois; tudo-ou-nada). Aba "Alteração em lote" no Estoque, filtro
  por qualquer critério (grupo, nome, fabricante, NCM, situação, sem margem, sem perfil fiscal, faixa de custo).
- **Perfil fiscal completo** (`mod-estoque::fiscal`, migração v8): origem, CST/CSOSN, CFOPs, alíquotas,
  redução de base, MVA-ST, CST PIS/COFINS, CEST, benefício — herdado produto → grupos acima; editável no
  grupo (aba Estrutura), no produto (Editar) e em lote (junto com o NCM).
- **Imposto do preço por produto** (Presumido/Real): `carga_do_produto` usa ICMS (com redução de base),
  PIS e COFINS do produto herdados do grupo; `SugestaoDePreco.imposto_origem` e etiqueta "do produto"
  na grade de Preços; margem prevista da OS usa o imposto de cada peça. Simples/MEI inalterados; ST/MVA não entram.
- **Painel de margem do pedido** (`vendas.margem_do_pedido.v1`, permissão nova `vendas.margem.ver`):
  receita, custo, margem bruta e líquida por item e no conjunto, descontando operação, imposto,
  comissão e taxas (os mesmos percentuais do preço sugerido); item sem custo fica fora do cálculo
  (nunca vira lucro de 100%); semáforo com texto no organismo novo `PainelMargem`; item abaixo da
  margem mínima ganha a etiqueta "margem baixa". Ainda não está no PDV.
- **Plano de contas** (`cardeal_ledger::modelos`, `mod-financeiro::plano`, migração financeiro v7): plano
  padrão pronto + **modelos por segmento** (comércio, serviços, assistência técnica), contas novas até 5
  níveis, e **categoria → conta** (o lançamento posta na conta própria; a DRE separa aluguel de energia).
  Aba **Plano de contas** com a árvore. Faltam importação por planilha e DE/PARA contábil.
- **Análise de estoque** (`mod-estoque::analise`): curva ABC por valor de saída, cobertura em dias,
  giro, dinheiro parado e risco de ruptura (aba **Análise**).
- **Score de saúde financeira** (`mod-financeiro::saude`, `financeiro.saude.v1`): cartão na Visão geral,
  cinco sinais determinísticos e explicáveis (ver a tabela em `docs/modulos/financeiro.md` §6).
- **Margem prevista da OS** (`os.margem_prevista_da_ordem.v1`, permissão nova `os.margem.ver`): o painel
  de margem no orçamento da OS — peças ao custo real (aplicadas), ao combinado da encomenda ou ao custo
  médio de hoje (marcado como estimativa) e **mão de obra sem custo, fora das margens** (o custo-hora do
  técnico ainda não existe). Substitui a linha "Margem" simples do detalhe.
- **Conciliação bancária** (`mod-financeiro::{extrato,sugestao,conciliacao}`, migração financeiro v8): importa
  OFX/CSV, sugere o casamento por valor exato (+ data e documento), concilia/lança/ignora; aba
  **Conciliação**. **Corrigido:** o DFC e o score ignoravam contas bancárias criadas pelo usuário
  (agora toda conta sob "1.1 Disponível", menos aplicações, conta como caixa).
- **Cobrança** (`mod-financeiro::aging`): aging dos recebíveis e pagáveis, taxa de inadimplência e lista
  de devedores com WhatsApp pronto — aba **Cobrança** (cena `financeiro-cobranca`).
- **Preço sugerido → tabela de preço:** `vendas.aplicar_precos_sugeridos.v1` (botão "Aplicar à tabela de
  preço…" na aba Preços) cria/atualiza a regra permanente do produto, com arredondamento para cima
  (,90 ou real inteiro); `mod_estoque::sugerir_precos` é a porta que `mod-vendas` usa.
- **Custo de referência + histórico de compras (2026-10-12):** parâmetro `custo_referencia` (custo médio |
  última compra | o maior dos dois; migração `estoque` v9) e consulta `estoque.historico_de_compras.v1`;
  na aba Preços, seletor no formulário e diálogo de compras ao clicar no produto.
- **Preços defasados (2026-10-12):** `vendas.precos_defasados.v1` classifica cada preço permanente da tabela
  (prejuízo / abaixo do alvo / no alvo / acima do alvo / sem cálculo) contra o preço que o estoque calcula
  hoje; seção na aba Preços com "aplicar o sugerido aos defasados" e aviso ao confirmar uma compra.
- **Rentabilidade por nível da árvore (2026-10-12):** aba **Rentabilidade** no Estoque — receita, custo, margem
  bruta e líquida **estimada** de cada Grupo/Subgrupo/Categoria/Subcategoria (subárvore e só os diretos) num período
  (mês, 30/90 dias, ano). Receita por produto vem de `vendas|pdv|os.receita_por_produto.v1` (permissões
  `vendas.margem.ver`, `pdv.margem.ver` — nova —, `os.margem.ver`), custo de `estoque.saidas_por_produto_e_origem.v1`,
  encargos de `estoque.encargos_atuais.v1`; o domínio puro é o crate novo `cardeal-analytics`
  (`rentabilidade::calcular`). Produto sem custo fica fora da margem; módulo sem permissão não soma. Pendente:
  comissão por estrutura; devolução/estorno ainda não abate o custo.
- **Seletor em árvore (2026-10-12):** componente `SeletorEmArvore` (organismo sobre `ArvoreHierarquia` +
  molécula `CampoSelecao`): o campo mostra o caminho escolhido, o painel traz a árvore com busca incremental.
  Usado no cadastro do produto (substitui o combo do grupo), no filtro de grupo das abas Preços e Alteração
  em lote e no destino "Mover para o grupo" do lote. "Mover para" da aba Estrutura segue com a lista de
  destinos válidos (precisa filtrar o que não cabe).
- **Importação da estrutura por planilha (2026-10-12):** `mod-estoque::importacao_estrutura` (domínio puro:
  parse CSV, plano de nós novos, erros por linha, idempotente), comando `estoque.importar_estrutura.v1` (tudo
  ou nada, permissão `estoque.produto.criar`) e prévia `estoque.previa_importacao_estrutura.v1`; aba Estrutura
  ganhou "Importar planilha…" (arquivo → prévia "N novos, M já existiam, K erros" → confirmar). Só cria.
- **Importação de produtos por planilha (2026-10-13):** `mod-estoque::importacao_produtos` (domínio puro: CSV
  com cabeçalho, colunas por nome e sinônimos, só `Nome` obrigatória; grupos que faltam pelo mesmo
  `ResolvedorDeCaminhos` da estrutura, extraído de `importacao_estrutura`; unidades novas por sigla; estoque
  inicial com custo e local; todos os problemas da linha numa mensagem; produto existente por GTIN ou nome
  fica como está, logo é idempotente), comando `estoque.importar_produtos.v1` (tudo ou nada; margem/reposição
  exigem `estoque.produto.editar` e estoque exige `estoque.movimento.entrada` via `ctx.concede`) e prévia
  `estoque.previa_importacao_produtos.v1`. Tela: "Importar produtos…" no topo do Estoque, com "Baixar
  modelo…". `normalizar_ncm` saiu de `Produto::novo` para a importação validar igual. Detalhes e limites em
  `docs/modulos/estoque.md`. Testes: 15 unit + 5 integração (Despachante + SQLite) + 1 de tela.
- **Comissão por estrutura mercadológica (2026-10-13):** migração `estoque` v10 (`comissao` no grupo, no produto
  e no antes/depois do lote); comandos `estoque.definir_comissao_do_grupo.v1`/`…_do_produto.v1`; herança
  produto → grupo → `ParametrosPreco.comissao` (agora "comissão padrão") em `BaseDePreco::comissao`, com
  `SugestaoDePreco.comissao`/`origem_comissao`. A comissão do produto entra no preço sugerido, nos painéis de
  margem do pedido e da OS e na rentabilidade (`cardeal_analytics::EncargosDaVenda`, receita somada por
  percentual de encargos: sem comissões próprias, resultado idêntico ao anterior). Telas: bloco "Comissão de
  vendedor" na aba Estrutura (o bloco de percentual herdado virou genérico, `bloco_herdado`, usado também pela
  margem), campo na alteração em lote, coluna na grade de Preços e na importação. A apuração veio em seguida
  (abaixo).
- **Apuração e pagamento da comissão do vendedor (2026-10-14):** migração `vendas` v2 (`comissao_percentual` no
  item, congelado no `FaturarPedido`; tabela `vendas_comissao`, um registro por pedido). `ApurarComissoes`
  (permissão nova `vendas.comissao.apurar`) é idempotente por pedido e lança D 5.42/5.5 / C 2.1.03 no fim do
  mês da venda; `PagarComissoes` quita o apurado de um vendedor no mês num lançamento só, de caixa/banco;
  `ComissoesDoMes` mostra apurado, pago e a **prévia** do que falta (mesmo cálculo da apuração). Evento
  `vendas.comissao_apurada.v1`. Tela: diálogo "Comissões" na tela de Vendas (mês, KPIs, grade por vendedor,
  apurar, pagar). Testes: 3 de domínio/resumo, 2 de integração (percentual congelado, idempotência, saldos no
  Razão, conta do modelo Comércio, recusas do pagamento) e 1 de tela. Fora: PDV (cupom sem vendedor) e
  ajuste por devolução.
- **Conferência do saldo bancário e alerta da conciliação (2026-10-14, onda 6):** `extrato::saldo_do_ofx` lê o
  `LEDGERBAL`; migração `financeiro` v9 (`financeiro_saldo_banco`); `InformarSaldoDoBanco` (saldo digitado) e
  `ConferenciaDasContas` (banco × Razão na data, diferença explicada por pendentes/ignoradas/baixas só no
  sistema, o que sobra "sem explicação", e os alertas de pendência antiga, extrato velho e diferença). Telas:
  painel na aba Conciliação e quadro na Visão geral. Testes: 3 de unidade, 1 de integração (a diferença
  explicada passo a passo: importar, conciliar, ignorar, saldo digitado) e 1 de tela.
- **Lançamento pré-preenchido (2026-10-14, onda 6):** `UltimoLancamentoAReceber`/`…APagar` (só títulos à mão
  ou de recorrência); o diálogo de lançamento preenche descrição, categoria e valor em branco ao escolher a
  pessoa, com aviso. Testes: 1 de integração (manual mais recente, título de compra fora, espécies separadas)
  e 1 de tela (preenche o vazio, não sobrescreve o digitado).
- **DFC indireto (2026-10-14, onda 6):** `cardeal_ledger::variacoes_patrimoniais` (contas patrimoniais fora do
  caixa, mesmo recorte da DRE) e `dfc::montar_indireto` (lucro da DRE + variações com o sinal trocado,
  classificadas por depreciação → grupo de fluxo → papel → código); `DfcIndiretoDoPeriodo`. Fecha com a
  variação do direto; a diferença de datas (competência × liquidação), se houver, vira linha visível. Tela:
  "Método direto / Método indireto" na aba DFC. Testes: 2 de unidade, 1 de integração (os dois métodos com o
  mesmo caixa e diferença zero) e a tela.
- **Conciliação com qualquer movimento da conta (2026-10-14):** `AlvoDaConciliacao::Movimento` (lançamento
  realizado na conta que não é baixa, sem linha conciliada; o par estornado que o banco não viu some) —
  migração `financeiro` v10. **Corrige** o depósito do caixa no banco (sangria) e transferências, que não
  tinham com o que casar, e a conferência do saldo, que os acusava como diferença. Preparação para os cheques
  (a compensação é um movimento assim). Testes: 1 de unidade e 1 de integração (depósito, par estornado,
  estorno de movimento já conciliado).
- **Cheques recebidos (2026-10-14, onda 6):** `mod_financeiro::cheque` (domínio: estados `EmCarteira →
  Compensado → Devolvido`, dados normalizados, alíneas) e `RegistrarCheque`/`CompensarCheque`/`DevolverCheque`/
  `CarteiraDeCheques` (migração `financeiro` v11). Receber dá a baixa em Cheques a receber (1.2.03), não no
  caixa; compensar é D Banco / C 1.2.03 e casa com o depósito na Conciliação; devolver estorna compensação e
  baixa (a parcela reabre) e publica `financeiro.cheque_devolvido.v1`. Tela: aba **Cheques** (KPIs, filtro,
  receber com busca da conta em aberto, "Compensou"/"Voltou"). Fora: cheque emitido (pagar com cheque),
  repasse/endosso a fornecedor e cheque como forma de recebimento no PDV. Testes: 3 de domínio, 1 de
  integração (o ciclo inteiro, com conciliação nos dois sentidos e reapresentação) e 1 de tela.
- **Balancete e diário (2026-10-14, onda 6):** implementados os dois previstos no contrato do Razão
  (`docs/contratos-internos.md`) e nunca feitos — `cardeal_ledger::{balancete, diario}` e as consultas
  `BalanceteDoPeriodo`/`DiarioDoPeriodo`. O balancete fecha por construção (competência); o resultado de anos
  anteriores, sem encerramento no sistema, aparece no PL. Tela: "Balancete" ao lado de DRE e DFC. Base do
  pacote do contador. Testes: 2 de unidade, 1 de integração (fechamento, ano anterior, diário) e a tela.
- **Pacote do contador (2026-10-14, onda 6):** crate novo `cardeal-planilha` (`.xlsx` próprio: ZIP armazenado com
  CRC-32 + Office Open XML; dinheiro, data, negrito, larguras, cabeçalho congelado), `cardeal_pdf::gerar_relatorio`
  (relatório tabular genérico com cabeçalho da empresa e quebra de página) e o botão "Pacote do contador…" na aba
  Resultado (pasta `Contador AAAA-MM` com PDF de DRE/DFC/balancete e planilha com essas tabelas + diário).
  Validação: testes do ZIP (lido de volta, CRC conferido), da planilha e do PDF; a amostra abriu no Excel
  (abas, formato, negrito, painel congelado); o pacote real passou no `System.IO.Packaging` do .NET. Envio
  automático (SMTP) fica para quando houver configuração de e-mail; Portal do Contador é da onda 8.
- **Régua de cobrança (2026-10-14, onda 6):** `mod_financeiro::regua` (régua padrão −3/0/1/7/15/30, etapa devida
  por parcela uma vez só, fila agrupada por cliente na etapa mais grave, mensagem redigida), migração `financeiro`
  v12, `ReguaDeCobranca`/`FilaDeCobranca`/`DefinirReguaDeCobranca`/`RegistrarCobranca` e duas permissões novas
  (`financeiro.cobranca.configurar`, `.enviar`). `cardeal-ui`: `Campo::multilinha` (com exemplo na galeria).
  Tela: "Cobranças de hoje" na aba Cobrança (WhatsApp registra o envio, "Pular", editor) e aviso na Visão geral.
  Testes: 4 de domínio, 1 de integração e 1 de tela.
- **Configuração fiscal sobre o `sefaz-api` (2026-10-14, onda 7, fatia 1):** `cardeal-fiscal` ganhou o leitor/
  escritor PKCS#12 em Rust puro (`ler_certificado_a1`/`preparar_certificado_a1`: PBES2 AES/3DES, PBES1
  3DES/RC2, MAC SHA-1/2, CNPJ pela extensão ICP-Brasil; regrava os PFX com RC2, que o OpenSSL 3 do `sefaz-api`
  recusa), o `ApiFiscalHttp` (Bearer + PFX/senha por chamada, numa thread própria) e o status do serviço na
  porta; `cardeal-storage` ganhou o `Cofre` (AES-256-GCM, chave fora da base); `cardeal-modkit`, os
  `Recursos` (`Ctx::recurso`, o `porta()` adiado); `mod-empresa`, os dados fiscais do emitente (IE, IM, endereço
  com IBGE); crate novo `mod-fiscal` (migração v1, `InstalarCertificado`/`RemoverCertificado`/
  `DefinirAmbienteFiscal`, `SituacaoFiscal`/`StatusDaSefaz`, `ProvedorFiscal`, `porta_da_empresa`). Desktop:
  Configurações › Fiscal e o aviso do certificado na Visão geral. Conferido em homologação contra
  `emissao.tensorroot.com` com o A1 da Tensorroot (SEFAZ-SP, a UF dela, e SEFAZ-SC: 107 para NF-e e NFC-e).
  Decisões: ADR-0019.
  Testes novos: 15 em `cardeal-fiscal` (mais o de homologação real, ignorado no CI), 3 do cofre, 1 dos
  recursos, 2 de dados fiscais, 2 + 3 de `mod-fiscal` e 2 de tela.
- **NFC-e do PDV (2026-10-15, onda 7, fatia 2):** `cardeal-modkit` ganhou **tarefas** (`Registro::tarefa`,
  `ContextoTarefa` com `ler`/`escrever`, `Despachante::executar_tarefa`; `Escritor`/`Leitor` com `Clone`
  barato; `ExecutorDeTarefas` no motor para rodar numa thread). `cardeal-fiscal`: tipos da NFC-e e do
  retorno, `emitir_nfce`/`consultar_documento` na porta (sefaz-api e simulado), `ChaveAcesso::montar` (DV
  módulo 11). `mod-fiscal`: tributação pelo perfil efetivo, montagem a partir do cupom, migração v2
  (documento, série por ambiente, CSC no cofre), `ConfigurarNfce`/`EmitirNfceDoCupom`/`ReenviarDocumento`,
  consultas da fila, a tarefa `fiscal.transmitir_pendentes` e `decidir` (a máquina de estados, pura).
  `mod-estoque::produtos_para_nota` e `mod-clientes::documentos_da_pessoa` públicos. Desktop: NFC-e
  automática no PDV com faixa de estado, área **Notas fiscais**, cartão NFC-e em Configurações › Fiscal e a
  transmissão em segundo plano (`fiscal_fundo`). Conferido na SEFAZ-SP de homologação pelo `sefaz-api`
  local: assinatura, schema e QR passaram; recusa só de cadastro (178 — a empresa mudou para o RS). Achados
  do teste real: rejeições com código < 200 vinham como "erro" (agora são rejeição) e um erro do emissor
  que se repete sai da fila depois de 6 tentativas. O `sefaz-api` publicado estava fora do ar
  (Cloudflare 1033) em 2026-10-15.
- **DANFCE, cancelamento e inutilização (2026-10-15, onda 7):** `cardeal-pdf` ganhou página de tamanho livre
  e `gerar_danfce` (80 mm, QR Code com `qrcodegen`); o desktop monta o cupom da nota guardada + XML e abre
  para imprimir (PDV e Notas fiscais). Porta: `cancelar`/`inutilizar` (sefaz-api e simulado). `mod-fiscal`
  migração v3 (tabela recriada: estado `Inutilizada`, `pedido`/`justificativa`/`protocolo_evento`),
  `CancelarNfce`/`InutilizarNumero` (permissão nova `fiscal.cancelar`) e `decidir_evento`; a mesma tarefa da
  fila leva os pedidos. Testes: cancelamento aceito, recusado por prazo, pedido repetido, inutilização,
  decisões dos eventos e o cupom (o QR renderizado decodifica com OpenCV).
- **Estorno da venda finalizada (2026-10-15, item 1 do doc 21):** `pdv.estornar_venda.v1` (permissão nova
  `pdv.venda.estornar`, Alto) estorna o lançamento da venda — achado pela origem com o novo
  `RepositorioRazao::lancamentos_vivos_da_origem` — e devolve ao estoque cada saída ao custo em que saiu
  (`mod_estoque::devolver_saidas_da_origem`, com `SaldoLocal::devolucao` aceitando custo zero). Estado
  `Estornado` em tabela à parte (`pdv_estorno`, migração v2 do PDV). `pdv.vendas_do_periodo.v1` alimenta o
  `F11` **Vendas de hoje** (estornar com supervisor, imprimir, anular NFC-e). No fiscal, migração v4
  (`anulada`) e `fiscal.anular_documento_da_venda.v1`: a fila cancela, inutiliza ou emite-e-cancela
  conforme o estado. Corrigido junto: o papel Operador de Caixa não tinha `fiscal.ver`/`fiscal.emitir`
  (a venda dele saía sem NFC-e).
- **Só compra vira compra (2026-10-15, item 2 do doc 21):** o leitor da NF-e passou a ler `tpNF`,
  `finNFe`, o destinatário e o CFOP de cada item; `classificar_para` recusa saída própria, outro
  destinatário, devolução de cliente e nota de entrada do emitente (importação por arquivo: erro com
  o motivo; varredura da SEFAZ: pula). CFOP fora de venda deixa aviso na nota (`compras` v5,
  `alerta`) e impede a confirmação automática.
- **Cancelar conta e alterar vencimento (2026-10-15, item 3 do doc 21):** `financeiro.cancelar_titulo.v1`
  (permissão nova `financeiro.titulo.cancelar`) estorna o `Confirmado` de cada parcela e marca
  título/parcelas cancelados — só para título avulso ou de recorrência e sem baixa válida;
  `financeiro.alterar_vencimento.v1` (`financeiro.titulo.editar`) muda o vencimento de parcela em
  aberto de qualquer origem. No diálogo da parcela, seções "Alterar vencimento" e "Cancelar esta conta"
  (com a explicação de onde desfazer quando a conta veio de outro módulo).
- **Saldo inicial, transferência e desativar conta (2026-10-15, item 4 do doc 21):**
  - `financeiro.definir_saldo_inicial.v1`: D conta / C Lucros acumulados, um só por conta, redefinir
    substitui;
  - `financeiro.transferir_entre_contas.v1` e `cancelar_transferencia` (migração v13,
    `financeiro_transferencia`): o caixa exige saldo, o banco aceita cheque especial;
  - `financeiro.desativar_conta.v1`: sem saldo, nunca as contas do sistema nem a de um caixa;
  - novo `RepositorioRazao::desativar_conta`;
  - permissões novas `financeiro.banco.editar` e `financeiro.banco.transferir`;
  - a aba Contas bancárias ganhou Transferir, Saldo inicial e Desativar por conta e a lista das
    transferências com Cancelar; a conta nova já pede o saldo de hoje.
- **Comprovante não fiscal e bobina de 58 mm (2026-10-15, item 5 do doc 21):**
  - `cardeal-pdf`: módulo `bobina` (80/58 mm, cursor comum) e `gerar_comprovante`; o DANFCE recebe a
    bobina;
  - `mod-pdv`: migração v3 (`pdv_preferencias`), `DefinirImpressaoPdv` / `ImpressaoDoPdv` e
    `ComprovanteDaVenda`, que monta empresa, itens, cliente e estorno no próprio PDV para o operador de
    caixa imprimir. Passou a depender de `mod-empresa` e `mod-clientes`, sem features padrão;
  - desktop: "Imprimir comprovante" na última venda e em Vendas de hoje (2ª via), comprovante
    automático opcional e a aba Configurações › Impressão. Conferido em imagem: o comprovante de 58 mm
    soma certo e fica legível.
- **Venda a prazo no PDV (2026-10-16, item 6 do doc 21):**
  - forma `Prazo` com `CondicaoPrazo`; o `pdv` v4 recria `pdv_pagamento_cupom` com a forma e a
    condição;
  - cliente obrigatório; crédito bloqueado ou acima do limite é recusado (novo
    `mod_financeiro::em_aberto_a_receber_do_cliente`);
  - o título a receber nasce ligado ao lançamento da venda (D Clientes a receber com o cliente);
  - o estorno cancela o título sem recebimento;
  - NFC-e `tPag 05` (`MeioPagamento::CreditoLoja`);
  - desktop: tecla `5` no pagamento com 1º vencimento e parcelas; o comprovante mostra "A prazo (Nx,
    1ª em …)" e a linha de assinatura.
- **Implantação por planilha (2026-10-16, item 7 do doc 21):**
  - `cardeal_kernel::planilha`: o leitor de CSV (`;`/`,`, aspas, BOM) e a `chave_coluna` saíram do
    estoque e agora servem a todas as importações;
  - `mod_clientes`: `importacao` (domínio), `ImportarPessoas` e `PreviaDaImportacaoDePessoas`, e
    `indice_de_pessoas` (documento → pessoa, nome → pessoas);
  - `mod_financeiro`: `importacao` (domínio), `ImportarContasEmAberto` e
    `PreviaDaImportacaoDeContas`, permissão `financeiro.titulo.importar`, origem `importacao`
    (cancelável) e a dependência nova em `mod-clientes` (núcleo pode depender de núcleo, doc 04
    §8) para achar ou cadastrar a pessoa na mesma transação;
  - desktop: `importacao_planilha.rs` (o comum), "Importar clientes…" em Clientes e "Importar contas
    em aberto…" no Financeiro, cada um com o modelo para baixar.
- **Desconto no total e cliente no PDV (2026-10-16, item 8 do doc 21):**
  - `mod_pdv`: `DescontoNoTotal` (percentual ou valor) no `Cupom`, `AplicarDescontoNoTotal`
    (`Shift+F5`), teto do papel para a venda inteira (`Cupom::conferir_teto`) e o rateio entre os
    itens na finalização; migração `pdv` v5 (`ALTER TABLE` na regra do cupom);
  - desktop: `tela_pdv/desconto_total.rs`, botão "Desconto no total" e o total da tela com a
    mesma conta do domínio; `F6` com busca por dígitos (documento com máscara, telefone) e
    "Cadastrar cliente" (o bloco de `pessoa.rs`);
  - `cardeal-auth`: Operador de Caixa com `clientes.pessoa.ver`/`.criar`;
    `PapelDeFabrica::limites_padrao` (teto de desconto: Admin/Gerente ∞, Caixa/Vendedor 10%);
  - `cardeal-motor`: `sincronizar_papeis_de_fabrica` completa os papéis de fábrica gravados
    (permissões e limites, só acrescentando), e a impressão do plano inclui a política deles.
- **Teto de desconto configurável (2026-10-20):** o padrão de fábrica fica (Admin/Gerente sem teto,
  Caixa/Vendedor 10%) e o dono ajusta por papel em Configurações › Usuários e acesso:
  - `mod_empresa`: `DefinirTetoDeDesconto` (`empresa.definir_teto_desconto.v1`) e
    `TetosDeDesconto` (`empresa.tetos_desconto.v1`, com quantos usuários têm o papel);
  - `RepositorioAuth::definir_limite` grava o limite inclusive em papel de fábrica (as permissões
    seguem imutáveis); a sincronização só preenche limite ausente, então o ajuste fica;
  - desktop: `tela_settings/tetos.rs`.
- **Compras: parcelas da nota e estorno da entrada (2026-10-20, item 10 do doc 21):**
  - `cardeal-fiscal`: `NotaFiscalXml.duplicatas` (`cobr/dup`: número, vencimento, valor);
  - `mod_compras`: migração v6 `compras_nota_parcela`; `ParcelaCompra` e `conferir_parcelas`;
    `ConfirmarEntrada.parcelas` (`confirmar_entrada_com_parcelas`); `EstornarEntrada`
    (`compras.estornar_entrada.v1`, `compras.entrada.estornar`); consultas `ParcelasDaNota` e
    `PreviaDoEstornoDaEntrada`; evento `compras.entrada_estornada.v1`;
  - `mod_financeiro`: `DadosLancamentoTitulo.cronograma`, `desfazer_titulo_da_origem_comum`,
    `situacao_do_titulo_da_origem`; `titulo_da_origem` prefere o título vivo;
  - `mod_estoque`: `retirar_entradas_da_origem`, `entradas_vivas_da_origem`,
    `SaldoLocal::estorno_de_entrada`;
  - desktop: `tela_compras/parcelas.rs` e `tela_compras/estorno.rs`; "Confirmar entrada" aparece
    na nota em conferência com tudo vinculado (esperava o estado `Conferida`, que nada alcança).
- **Prazo de cancelamento visível (2026-10-20, item 12 do doc 21):** `ModeloFiscal::prazo_de_cancelamento_segundos`
  e `DocumentoResumo::prazo_de_cancelamento` → `mod_fiscal::PrazoDeCancelamento`; o detalhe em Notas
  fiscais e o `F11` do PDV mostram o tempo e avisam fora do prazo, sem bloquear.
- **XMLs no pacote do contador (2026-10-20, item 11 do doc 21):**
  - `cardeal-planilha`: o `zip` vira público, com `empacotar_comprimido` (deflate do `miniz_oxide`, que
    já estava no grafo; erro em vez de pânico acima de 65.535 arquivos ou 4 GiB);
  - `cardeal-fiscal`: o `RetornoSefaz.xml` do cancelamento e da inutilização é o XML do evento; o
    simulado o devolve, e o `nfeProc` dele tem `dhEmi`;
  - `mod_fiscal`: migração v5 (`xml_evento`), `Mudanca.xml_evento`, `NotasFiscaisDoMes`
    (`fiscal.notas_do_mes.v1`), `NotaDoMes`, `SituacaoNoMes`, `XmlQueFalta`, `dia_da_emissao`;
  - `mod_compras`: migração v7 (`xml`), a importação guarda o XML original (e o reimportar preenche o
    que faltava), `importar_nota_da_sefaz` recebe o XML bruto, `NotasDeEntradaDoMes`
    (`compras.notas_do_mes.v1`);
  - desktop (`pacote_contador.rs`): `XMLs AAAA-MM.zip`, as seções "Notas fiscais do mês" e
    "Pendências do mês" no PDF, e as abas "Notas emitidas" e "Notas de entrada" na planilha;
  - `cardeal-pdf`: o sinal de menos (U+2212) sai como traço, não como "?".
- **NF-e de venda (2026-10-27, fiscal.md §11 item 5, primeira parte):** `cardeal_kernel::municipio` (a
  tabela do IBGE embutida — o código do município pelo nome, sem rede); `cardeal-fiscal`: `Nfe`,
  `Destinatario`, `IndicadorIe`, `PortaFiscal::emitir_nfe` (HTTP `/nfe/emitir` e simulado); `mod-fiscal`:
  `nfe_venda.rs` (`EmitirNfeDoPedido`, `DocumentoDoPedido`, `ConfigurarNfe`, `ConfiguracaoDaNfe`), migração
  v10, `montar_nfe`/`destinatario_do_cliente`/`cfop_interestadual`, a fila pelos dois modelos (NF-e sem
  contingência nem substituição), `DetalheDocumento.nfe`; desktop: Emitir NF-e no pedido faturado e a série em
  Configurações › Fiscal. O DANFE A4 veio em seguida (`cardeal_pdf::danfe`, Code 128C próprio em
  `cardeal_pdf::barras`, conferido com o zxing; desktop `danfe.rs`).
- **Carta de correção da NF-e (2026-10-27, fiscal.md §7):** `cardeal-fiscal`:
  `PortaFiscal::carta_de_correcao` (HTTP `/nfe/carta-correcao` com a sequência, e simulado); `mod-fiscal`:
  `correcao.rs` (`EmitirCartaDeCorrecao`, `CartasDeCorrecao`, `decidir_carta`), migração v11, a fila leva as
  cartas depois das notas, `ResumoFiscal.cartas_pendentes` (a vigia de fundo conta), `NotasDoMes.cartas`;
  o pacote do contador leva as cartas e dá o nome 110112 ao cancelamento por substituição; desktop: Carta
  de correção no detalhe da NF-e.
- **Devolução ao fornecedor + NF-e de devolução (2026-10-27, compras.md §5, fiscal.md §7):** `sefaz-api`
  8075fde (`impostoDevol`, FCP do ST, o vNF com os dois; a validação do ST pelo teto); `cardeal-fiscal`:
  `TributosDaCompra` (o leitor do XML guarda ICMS/ST/FCP-ST campo a campo), `DevolucaoDoItem` e
  `Nfe.devolvidos`; `mod-estoque`: `devolver_parte_da_entrada`; `mod-financeiro`:
  `abater_titulo_da_origem_comum`; `mod-compras`: `devolucao.rs` (`DevolverAoFornecedor`,
  `EmitirNfeDaDevolucao`, `DevolucoesDaNota`, `dados_da_nfe`), migração v9, o estorno recusa nota com
  devolução; `mod-fiscal`: `devolucao_compra.rs` (`montar_nfe_de_devolucao`,
  `emitir_nfe_de_devolucao_comum`, `DocumentoDaDevolucao`); desktop: Compras › nota › Devolver ao
  fornecedor, a lista das devoluções com a NF-e (emitir, situação, DANFE) e o DANFE com ST/IPI devolvidos.
- **ICMS-ST estimado pela MVA (2026-10-28, compras.md §5, fecha a pendência da onda 5):** `mod-compras`:
  `st.rs` (`mva_ajustada`, `estimar_st`, `EstimativaDeStDaNota`), migração v10 (`st_estimado`),
  `ConfirmarEntrada.incluir_st_estimado` (o custo e a guia `compras_st`), o estorno que desfaz, a confirmação
  automática que para; desktop: o quadro "ICMS-ST estimado pela MVA" na confirmação da entrada.
- **Calendário de obrigações (2026-10-28, fiscal.md §7):** `mod-fiscal`: `calendario.rs` (`pascoa`,
  `feriados`, `prorrogar`/`antecipar`/`dia_util`/`ultimo_dia_util`, `obrigacoes_do_mes` por regime,
  `ObrigacoesDoMes` com as do Cardeal); desktop: o cartão "Obrigações" na Visão geral
  (`tela_financeiro/obrigacoes.rs`).
- **NFS-e da OS (2026-10-28, fiscal.md §7):** `sefaz-api` 5726b25 (a emissão no ADN lê `chaveAcesso`/
  `nfseXmlGZipB64`; a consulta pelo DPS busca a nota pela chave); `cardeal-fiscal`: `nfse.rs` (`Nfse`,
  `TomadorNfse`, `RetornoNfse`, `CancelamentoNfse`), `PortaFiscal::emitir_nfse`/`consultar_nfse_por_dps`/
  `cancelar_nfse` (HTTP e simulado); `mod-fiscal`: `nfse.rs` + `nfse/sqlite.rs` (`ConfigurarNfse`,
  `ConfiguracaoDaNfse`, `EmitirNfseDaOs`, `NfseDaOs`, `CancelarNfse`, `montar_nfse_da_os`, `decidir_nfse`), migração
  v12, a fila leva as NFS-e depois das cartas, `NotasDoMes.nfse`; `mod-fiscal` passa a depender do `mod-os`;
  desktop: Configurações › Fiscal › NFS-e e OS faturada › NFS-e (emitir, situação, cancelar), a vigia e o
  pacote do contador.
- **Devolução de venda + NF-e de entrada + ajuste de comissão (2026-10-27, vendas.md §5, fiscal.md §7):**
  `mod-vendas`: `comandos/devolucao.rs` (`SolicitarDevolucao`, `AprovarDevolucao`, `CancelarDevolucao`,
  `ConcluirDevolucao`, `DevolucoesDoPedido`, `DevolucoesPendentes`, `devolucao_por_id`), migração v3
  (`vendas_devolucao`, `vendas_devolucao_item`, `vendas_comissao_ajuste`), `Devolucao::solicitar_descontando`,
  o receituário com o reembolso opcional e `estornar_comissao`, a apuração líquida de devoluções, o pagamento
  que desconta os estornos, `LinhaComissao.devolucao`; `mod-estoque`: `devolver_parte_da_saida`;
  `mod-financeiro`: o abatimento serve as duas espécies e sai sem dinheiro; `mod-fiscal`:
  `devolucao_venda.rs` (`EmitirNfeDaDevolucaoDeVenda`, `DocumentoDaDevolucaoDeVenda`,
  `cfop_da_devolucao_de_venda`), o Reenviar remonta essa nota; `cardeal-auth`: o Vendedor pede devolução;
  desktop: Vendas › pedido faturado › Devoluções (pedir, aprovar, concluir, cancelar, NF-e de entrada, DANFE).
- **Substituição da NFC-e sem resposta (2026-10-27, fecha o fiscal.md §11 item 3):** `sefaz-api`
  43c98f0 (o `/nfe/cancelar` com `chaveSubstituta` = evento 110112); `cardeal-fiscal`
  `PortaFiscal::cancelar_por_substituicao`; `mod-fiscal`: migração v9, `pede_substituicao`,
  `nao_reemitir`, `pedido_da_substituida`, o pedido `CancelarPorSubstituicao`, a fila em funções
  menores (`processar`, `decidir_documento`, `criar_substituta`). A substituta sai sempre em
  contingência; a original é cancelada por substituição ou inutilizada, nunca emitida de novo.
- **Série da NFC-e por caixa (2026-10-27, fiscal.md §11 item 4):** `mod-fiscal` `terminal.rs`
  (`DefinirSerieDoTerminal`, `SeriesDosTerminais`, migração v8), a emissão pela série do terminal do
  cupom, `ConfigurarNfce` recusando a série de um caixa; desktop: `maquina::terminal()` (a
  identidade persistente da máquina) no PDV e Configurações › Fiscal › Série da NFC-e por caixa.
  Achado: o terminal do PDV era um `Id::novo()` a cada abertura do app — o número da venda
  recomeçava do 1 toda vez.
- **Impressão direta na térmica (2026-10-27, fiscal.md §11 item 2):** `cardeal-pdf` ganhou a interface
  `Escrita` (o layout da bobina escreve nela; `Cursor` é o PDF) e o `escpos` (`danfce_escpos`,
  `comprovante_escpos`, `teste_escpos`, `OpcoesEscPos`; QR em `GS v 0`, PC860 opcional). Desktop:
  `maquina` (a preferência desta máquina em `maquina.json`), `impressora` (spooler do Windows em
  modo cru via `windows-sys`, `lp -o raw` fora dele, TCP 9100; `instaladas()`), Configurações ›
  Impressão › Impressora deste computador, e o "imprimir sozinho" passa a imprimir o cupom fiscal
  quando a NFC-e sai. Não conferido numa térmica de verdade.
- **Contingência off-line da NFC-e (2026-10-27, fiscal.md §11 item 3):** `sefaz-api` 7d7faa6 (o
  `naoEnviado` — o pedido que com certeza não chegou à SEFAZ); `cardeal-fiscal`: `RetornoSefaz::nao_enviado`
  e `sefaz_fora`, `PortaFiscal::preparar_nfce_em_contingencia`/`transmitir_nfce_em_contingencia`
  (HTTP e simulado, `simular_sefaz_fora`); `mod-fiscal`: estado `Contingencia` (migração v7),
  `pede_contingencia`/`em_contingencia`, transmissão do mesmo XML, resumo e faixa com o prazo de 24 h;
  `cardeal-pdf`: o aviso no DANFCE; desktop: imprimir o cupom da contingência no PDV, nas vendas do dia e
  nas Notas fiscais. Achado: a vigia de fundo só olhava pendentes e incertas. Pendente: a nota incerta em
  contingência com outro número (cancelamento por substituição, 110112).
- **Score e curva de clientes (2026-10-26, fecha o item 13 do doc 21):** `mod_financeiro::pagamento`
  (`avaliar`, `PerfisDePagamento`, `perfis_de_pagamento` — só a prazo, tolerância de 3 dias, nota
  0–1000, selos), `mod_vendas::vendas_por_cliente`, `mod_pdv::CurvaDeClientes` (`montar_curva`:
  ABC relativa, ritmo, Sumido/Novo/Ativo/Esporádico, ação), permissão nova `pdv.clientes.ver`
  (negada ao Operador de Caixa); desktop: Clientes › "Curva de clientes" e o selo de pagamento na
  ficha. Desvio do `clientes.md`: o score é calculado na hora no financeiro, não gravado por evento.
- **Orçamento por conta (2026-10-26, item 13 do doc 21, segunda parte):** `mod_financeiro::orcamento`
  (`OrcamentoDoAno`, `DefinirOrcamentoDaConta`, `CopiarOrcamentoDoAno`, `desvio` no sentido do
  resultado), migração `financeiro` v14 (`financeiro_orcamento`), permissão
  `financeiro.orcamento.editar`; `ComparativoDre.orcado`; `dre::montar_mantendo`/`linha_orcada`;
  `cardeal_ledger::contas_de_resultado`; desktop: colunas "Orçado"/"Real × orçado" na DRE e a visão
  Resultado › Orçamento.
- **Resumo do Simples (2026-10-26, item 13 do doc 21, primeira parte):** `mod_estoque::simples`
  (puro: `rbt12` pela lei — 12 meses anteriores, anualização do primeiro ano, histórico curto —,
  `resumir`, avisos de sublimite/limite/MEI), migração `estoque` v14 (`estoque_simples_historico`,
  `estoque_simples_receita_anterior`), `ResumoDoSimplesDoMes`, `HistoricoDoSimplesDaEmpresa`,
  `DefinirHistoricoDoSimples`, permissão nova `estoque.preco.simples.ver`; o preço sugerido passa a
  usar o mesmo RBT12; desktop: Financeiro › Resultado › "Simples Nacional".
- **Notas de compra pela SEFAZ — DFe (2026-10-26, item 9 do doc 21):**
  - `sefaz-api` (842576f): `/nfe/dfe/distribuicao`, `/nfe/dfe/consulta-chave`, `/nfe/manifestacao`;
  - `cardeal-fiscal`: `PortaFiscal::distribuicao_dfe(cnpj, uf, desde) -> PaginaDfe`,
    `consultar_dfe_por_chave`, `manifestar`; `ler_resumo_nfe`/`ler_evento`; saem `ResumoDfe`,
    `XmlNota` e `baixar_xml`; o simulado entrega como a SEFAZ (resumo, ciência, NSU novo, 656);
  - `mod-fiscal`: `ambiente_da_empresa`;
  - `mod-compras`: migração v8 (`compras_dfe_estado` por ambiente, `compras_dfe_nota`), a tarefa
    `compras.buscar_notas_na_sefaz`, `NotasNaSefaz`, `HoraDeBuscarNaSefaz`, `ImportarDeNovoDaSefaz`;
    sai `verificar_notas_na_sefaz` (rodava a rede dentro de uma transação); passa a depender de
    `mod-fiscal` e `mod-empresa` (só com o SQLite);
  - desktop: `fundo::TarefaDeFundo` (a transmissão fiscal passou a usar), `compras_fundo` (a vigia de
    minuto em minuto), e em Compras o botão "Buscar na SEFAZ" e a seção "Notas que chegaram pela
    SEFAZ".
- **XML do evento e busca do que falta (2026-10-26, fecha o item 11 do doc 21):**
  - `sefaz-api` (bc58e77, outro repositório): `procEventoNFe` no cancelamento e na CC-e,
    `procInutNFe` na inutilização, `xmlAssinado` no desfecho incerto, o `nfeProc` da denegada, e na
    consulta `xml` (com o `xmlAssinado`, conferido pela chave e pelo digest) e `xmlCancelamento`; a
    consulta da cancelada volta `cancelado` com o 101. Testes com a assinatura conferida dentro do
    envelope;
  - `cardeal-fiscal`: `RetornoSefaz.xml_assinado`/`xml_cancelamento`; `consultar_documento` leva a
    nota assinada; o simulado imita tudo isso;
  - `mod_fiscal`: migração v6 (`xml_assinado`, `xml_a_buscar`); `Mudanca.xml_assinado`/`buscar_xml`;
    `decidir_busca` e `Busca` (puras); a fila busca os XMLs de documentos fechados sem mudar o estado;
    `BuscarXmlsQueFaltam` (`fiscal.buscar_xmls_que_faltam.v1`); `ResumoFiscal::xmls_a_buscar`;
    `XmlQueFalta::NotaABuscar`;
  - desktop: o pacote do contador pede a busca e dispara a fila; a vigia dispara quando há busca na
    hora;
  - achado: um texto das pendências saía com o recuo do código no meio (faltava a continuação de
    linha) — corrigido, com teste.
- **Inventário com contagem cega (2026-10-13):** migração `estoque` v11 (`estoque_inventario`,
  `estoque_contagem_item`), comandos `criar_inventario`/`iniciar_contagem`/`registrar_contagem`/
  `encerrar_inventario`/`cancelar_inventario` (`.v1`) e consultas `inventarios`/`posicao_do_inventario`.
  Iniciar tira a foto do saldo e trava o local na mesma transação; o corpo comum de entrada/saída e o
  `AjustarSaldo` recusam movimento no local travado (`RECURSO_TRAVADO`) — **inclusive a venda**, por regra do
  spec. Contagem cega (a posição só revela sistema/diferença/valor depois de encerrado), item achado entra na
  hora, encerrar ajusta saldo + razão por `aplicar_delta_de_saldo` (extraído do `AjustarSaldo`, agora o corpo
  comum dos dois). `CancelarInventario` e o estado `Cancelado` foram acrescentados ao spec. Aba **Inventário**
  no Estoque. Testes: 2 de domínio + 4 de integração + 1 de tela.
- **Transferência entre locais (2026-10-13):** domínio `mod-estoque::transferencia`, migração v12, comando
  `estoque.transferir_entre_locais.v1` (vários itens, tudo ou nada, ao custo médio da origem) e consultas
  `estoque.transferencias.v1`/`estoque.saldos_do_produto_por_local.v1`. Imediata (a etapa em trânsito com
  confirmação fica para a transferência entre CNPJs com NF-e), recusa sem saldo, recusa produto com lote
  ativo na origem, sem lançamento no razão (uma conta de estoque por CNPJ). Aba **Transferências** no Estoque.
  Testes: 2 de domínio + 3 de integração + 1 de tela.
- **Produto a partir da nota + dicionário de nomes (2026-10-13):** `mod-estoque::padronizacao` (sugestão
  determinística do nome a partir do `xProd`: capitalização, DE/PARA, conteúdo normalizado no fim; e o
  aprendizado só do que dá para explicar), migração `estoque` v13 (`estoque_dicionario_nome`), comandos e
  consultas do dicionário e `criar_produto_de_compra_comum`; em `mod-compras`, migração v3 (o `cEAN` fica no
  item) e `compras.criar_produto_do_item.v1` (cria, vincula, aprende regra de casamento e dicionário). Telas:
  "Criar produto novo" no item não casado da conferência e "Dicionário de nomes…" no Estoque. Falta a
  descrição reduzida (PDV) e o template configurável de ordem. Testes: 5 de domínio + 2 de integração + 2 de
  tela (o primeiro teste da tela de Compras).
- **Validade e perdas (2026-10-13):** `estoque.registrar_perda.v1` (motivo + observação no histórico, sai do
  lote e do saldo, esgota o lote, D Perdas / C Estoque **ao custo médio** — como toda saída; recusa perda acima
  do saldo), `estoque.lotes_com_validade.v1` (vencidos e a vencer, vencimento calculado na hora) e código de
  lote gerado quando em branco em `registrar_entrada_com_lote`. Aba **Validade** no Estoque e cartão de alerta na
  aba Produtos. Pendentes: evento/marcação diária de `Vencido` (agendador), lote na confirmação da nota (`rastro`
  do XML) e FEFO na venda. Testes: 2 de integração + 1 de tela.
- **Custo-hora do técnico (2026-10-13):** `mod_os::custo_hora` (tabela por técnico/usuário + padrão da oficina),
  migração `os` v9, `os.definir_custo_hora.v1` (permissão nova `os.custo_hora.definir`) e `os.custos_hora.v1`.
  A margem prevista da OS custeia a mão de obra (horas previstas ou apontadas × custo-hora); a margem real por OS
  e por período desconta o tempo apontado (`custo_mao_de_obra`, `mao_de_obra_sem_custo`). Gerencial: o razão não
  muda (a mão de obra própria é despesa de pessoal). Telas: "Custo-hora da equipe…" na OS, "Horas previstas" no
  orçamento, mão de obra no diálogo de baixa e no cartão do Financeiro. Testes: 3 de domínio + 1 de integração + 1
  de tela.
- **ICMS-ST, IPI e desconto da nota no custo (2026-10-13):** o leitor da NF-e lê `vDesc`, `vICMSST`+`vFCPST` (não
  o `vICMSSTRet`) e `vIPI` por item; `mod-compras` (migração v4) guarda desconto e impostos no item, o custo de
  entrada vira bruto − desconto + rateio + ST + IPI e o total da nota passa a ser o `vNF`. **Corrige um erro real:**
  o título a pagar saía menor que o boleto em toda nota com ST ou IPI. A conferência mostra a composição do
  custo. Testes: 1 do leitor + 1 de integração (custo médio e título com o `vNF`).
- **Dívidas conhecidas:** tabelas do Simples a conferir com o contador; a 6ª faixa (ver ADR-0018; a
  anualização do RBT12 foi feita em 2026-10-26); o PDV só avisa preço abaixo do mínimo de empate (etiqueta "abaixo do mínimo" na linha do
  cupom, depois do desconto); não bloqueia a venda e não é o painel de margem completo.

## 2. Decisão estratégica registrada: por que Rust, não C#/.NET

Em 2026-09-02 avaliamos um projeto irmão do mesmo autor, **SistemaX** (`../sistemax`), um ERP
em .NET 10 + React/WebView2 com a *mesma tese* (financeiro como coração, eventos de
integração idempotentes, FSM explícita, plugin de módulo sem `if` sobre módulo concreto).
A avaliação foi honesta e favorável ao SistemaX em vários pontos — arquitetura de domínio
madura, 1.075 testes reais, motor fiscal próprio testado para os 3 regimes. Mas:

1. **O HEAD do SistemaX não compila** — `IBackupManager`/namespace `Backup` referenciado em
   4 arquivos desde o commit inicial, nunca implementado. Não há CI contínuo (só dispara em
   tag de release), e o próprio guia de processo do projeto manda validar build só "nos
   projetos tocados" — exatamente o buraco que deixou uma peça transversal quebrada por
   27 commits sem ninguém notar.
2. **WebView2 + React tem um teto físico de RAM (~200–400 MB)** que nenhuma quantidade de
   bom código C# resolve — é escolha de stack, não bug. Isso conflita direto com o Pilar I
   deste projeto (doc 02): meta de ~90 MB no terminal PDV.
3. O motor fiscal próprio do SistemaX — seu maior trunfo concreto — deixou de ser um
   diferencial relevante quando o usuário decidiu usar uma **API externa** (`sefaz-api`,
   também em `Develop/`) para emissão fiscal nos dois projetos.

**Decisão:** seguir com Cardeal. Ver a conversa completa para o raciocínio detalhado; este
parágrafo existe para que ninguém reabra essa discussão sem contexto.

Nota à parte, sem relação com a decisão acima: o `sefaz-api` tem um `AUDIT_REPORT.md`
documentando **segredos vazados no histórico do git** (chave privada do Firebase admin +
chave AES-256 de descriptografia de certificado A1, commit `695d473`) que **ainda precisam
ser rotacionados**. Isso é urgente e independente de qualquer decisão de arquitetura.

## 3. O processo que este projeto segue

Isto é o que muda de "só código" para "processo replicável". Uma IA nova (ou você, em três
meses) deve seguir isto, não reinventar.

### 3.1 Documentação antes de código

A arquitetura inteira (docs 00–18, ADRs, specs de módulo, `contratos-internos.md`) foi
escrita **primeiro**, por completo, antes de qualquer linha de Rust. Código implementa o que
já está especificado; não inventa arquitetura ad hoc. Quando um crate precisa de uma decisão
que a documentação não cobre, a decisão é tomada e **a documentação é atualizada no mesmo
lote de trabalho** — nunca só no código.

### 3.2 Domínio primeiro, storage por último

Todo crate novo começa pelo **domínio puro**: tipos, invariantes, regras — sem tocar banco,
sem `async`, sem I/O. Onde o domínio precisaria de persistência, ele recebe uma **trait
mínima local** (porta) em vez de esperar o crate de storage existir — exemplo real:
`cardeal_ledger::PortaRazao`, que `cardeal-storage` vai implementar quando nascer, mas que já
permite testar `Razao::registrar`/`estornar`/`confirmar`/`liquidar` em memória, hoje. Isso
está documentado no próprio `lib.rs` de cada crate, na seção "o que falta".

**Por quê:** permite construir e provar corretas as partes centrais (o Razão, a
autenticação) fora de ordem de dependência, sem bloquear em uma peça de infraestrutura que
ainda não existe — e sem gerar código descartável quando ela existir (a porta vira o ponto
de encaixe, não é jogada fora).

### 3.3 Nunca commitar sem verificar o workspace inteiro

Antes de qualquer commit:

```bash
cargo check --workspace     # a TOTALIDADE do workspace, não só o crate tocado
cargo test -p <crate-tocado>
cargo fmt -p <crate-tocado>
cargo fmt --check -p <crate-tocado>
```

Esta é a lição direta do §2: o SistemaX ficou quebrado desde o commit 1 porque o processo
dele só validava "os projetos tocados". Aqui, `cargo check --workspace` roda sempre — mesmo
quando o crate tocado é pequeno, mesmo quando parece óbvio que não afeta o resto.

### 3.4 Todo tipo novo com invariante ganha teste — e propriedade, quando fizer sentido

Toda regra documentada (soma zero do lançamento, bloqueio progressivo, dígito verificador de
CPF/CNPJ) tem teste unitário provando o caso concreto **e**, quando existe uma propriedade
algébrica (ex.: "qualquer sequência de partidas só constrói se a soma for zero"), um teste
`proptest` cobrindo centenas de casos gerados. Criptografia real (Argon2id) é testada de
verdade — nunca mockada.

### 3.5 Consciência de custo (tokens, tempo, subagentes)

Este projeto foi construído sob restrição explícita de custo em vários pontos da conversa.
Práticas adotadas:

- Implementação direta (sem subagente) para trabalho sequencial bem definido — mais barato e
  sem risco de rate limit de sessão de outro modelo.
- Escopo pequeno por lote: um crate ou uma fatia clara de domínio por vez, sempre fechado
  (compila, testa, formata, documenta) antes de começar o próximo.
- Quando subagentes Sonnet foram usados para tarefas grandes (ADRs, specs de módulo), o
  prompt trazia todo o contexto necessário para não exigir exploração cara.

### 3.6 Convenção de commit

Mensagens em português, explicando o **porquê**, não só o que mudou; corpo lista o que foi
testado. Um commit por incremento coerente (um crate, uma feature de domínio) — não um commit
gigante por sessão. Nenhum commit é feito sem os quatro comandos de §3.3 passarem.

## 4. Onde retomar

1. Confirme o estado: `cargo check --workspace && cargo test --workspace` (os stubs não têm
   teste, então isso deve ser rápido e 100% verde).
2. Releia `docs/contratos-internos.md` — é o contrato normativo entre crates (a §4
   `cardeal-modkit` e a §5 `cardeal-auth` já estão preenchidas, incluindo o despacho e o
   `RepositorioAuth`). Qualquer trabalho novo deve implementar exatamente essas assinaturas.
3. Próximo passo natural — usuário confirmou a ordem: **OS primeiro** (feito), **compras
   depois** (feito — importação automática de nota via SEFAZ simulada, ver §1.1/§1.2),
   **PDV/fiscal por último** (o usuário já tem certificado A1 e `sefaz-api` rodando, então
   dá para testar de verdade em homologação quando chegar lá). O que falta em compras, para
   quando tiver consumidor: cotação/pedido de compra formal (hoje vai direto da nota ao
   estoque, como o `gestao-raiz` faz), devolução ao fornecedor, casamento por GTIN e leitura
   de condições de pagamento do XML (`cobr/dup` — hoje `ConfirmarEntrada` sempre gera parcela
   única vencendo na emissão). Próximo passo real:
   - **`cardeal-fiscal` + `mod-fiscal`** (fase atual): hoje `cardeal-fiscal` só tem o parser
     de XML (`interpretar`, `quick-xml`, real) e `FiscalSimulado` (double em memória — é o
     que `mod-compras` usa em teste). Falta o adaptador de verdade: certificado digital A1
     (upload, parse do PKCS#12,
     validade — **resolvido em 2026-10-14**: o que o TLS do `sefaz-api` (OpenSSL 3) recusa é
     PFX com **RC2**, não AES; o Cardeal regrava só esses na instalação), `PortaFiscal`/`ApiFiscalHttp` mirado no
     contrato real do `sefaz-api` (Bearer token, PFX+senha no corpo de cada chamada de
     emissão — o serviço não guarda certificado —, NFC-e com contingência offline real via
     `/prepare`+`/transmit`, NF-e só lote+polling sem contingência própria). Tradução
     amigável de `cStat` (autorizado/rejeitado/denegado/duplicidade) segue o modelo do
     `gestao-raiz` (`classifySefazResponse`), não o código. Nunca reemitir sem antes
     consultar por `chaveAcesso` — o `sefaz-api` não tem idempotency key na emissão de NF-e.
   - ~~**`mod-pdv`**~~ **feito** (ver §1.1) — a venda de balcão em si não depende de fiscal;
     só a emissão de NFC-e depende (e essa continua fora do escopo do PDV por desenho, sempre
     assíncrona via `mod-fiscal`).
   - ~~**`mod-vendas` no despacho**~~ **feito** (ver §1.1) — `ConfirmarPedido` só faz a
     transição de estado, sem reserva de estoque de verdade (`ReservarEstoque`/
     `LimiteDisponivel` não existem ainda em `mod-estoque`/`mod-clientes`); orçamento,
     devolução, comissão e contrato recorrente também ficaram de fora desta fatia (domínio
     puro já pronto, só falta o comando).
   - ~~**Financeiro: `EstornarBaixa`/`RenegociarTitulo`**~~ **feito** (ver §1.1) — os dois de
     risco Alto que faltavam da Fase 1. Próximo no financeiro: o Pulso "incrível" com
     Realizado/Comprometido/Previsto puxando
     compromissos de compras/vendas/OS em aberto — via consulta pública de cada módulo
     (nunca leitura direta de tabela, `docs/contratos-internos.md` §7 regra 2), não via
     barramento de evento (que ainda nem existe). O algoritmo de projeção diária do
     `gestao-raiz` (soma recebível−pagável dia a dia, guarda o menor saldo acumulado e a
     data) é um bom esqueleto, mas ele descrito lá é "rudimentar" — não liga estoque baixo
     nem OS em andamento à projeção; isso é para desenhar do zero.
   - **`cardeal-server`: a montagem de `AutorizacoesEfetivas`** — cruza os papéis do usuário
     (`RepositorioAuth::papeis_do_usuario`) com o `ConjuntoEfetivo` da empresa
     (`RegistroModulos::resolver`) e emite a `Sessao`. É a camada que tem `cardeal-auth` **e**
     `cardeal-modkit` (o inverso fecharia ciclo — o despacho chama `autorizar`).
   - **`cardeal-auth`: `Dispositivo` + `Sessao` persistidos** sobre `nucleo_dispositivo`/
     `nucleo_sessao` (token rotativo, §4.2) — encosta no transporte, pode vir com
     `cardeal-protocol`.
   - **`cardeal-ledger`: um resolvedor de contas não-genérico** (`ContasResolvidas` — um
     snapshot `papel → Id`) para destravar `Ctx::contas()`.
   - Em `cardeal-storage`, o que ficou para depois: `Outbox` (leitura das pendências),
     `Backup` online, `verificar()`, checkpoint na ociosidade, feature `cripto`, timeout por
     tarefa.
   - Em `cardeal-modkit`, o que ficou para depois no despacho: assinaturas de evento,
     tarefas agendadas, portas, itens do Pulso, `ao_ativar`/`diagnostico`, replay de
     idempotência, auditoria automática de `AUDITA`, escopo por-comando.
4. Os 5 specs de módulo que faltam (`alugueis`, `combustivel`, `hotelaria`, `industria`,
   `contabil`) são de baixa prioridade — nenhum perfil que os usa está no caminho crítico
   ainda (ver doc 17, Fase 5).
5. `cardeal-fiscal` deve ser desenhado como cliente HTTP fino do `sefaz-api`
   (`../sefaz-api`) — não como emissor próprio; detalhes do contrato real já levantados no
   item 3 acima. **Segurança, não é sobre o Cardeal, mas é urgente**: o `AUDIT_REPORT.md`
   do `sefaz-api` confirma que a chave AES-256 que o `../gestao-raiz` usa para criptografar
   senha de certificado A1 vazou no histórico do git dele (junto com uma chave privada do
   Firebase), ainda não rotacionada. Quando o Cardeal implementar o armazenamento do
   certificado (via `cardeal_storage::Segredo<T>`), gerar chaves **novas**, isoladas desse
   histórico — nunca reusar nada de lá.

## 5. Não repita isto

- **Não construa UI com `egui` cru numa tela.** Toda tela (`crates/cardeal-desktop/src`) é
  composta só por componentes de `cardeal-ui` (Atomic Design). Sem `egui::Frame`, `Stroke`,
  `Color32`, `ui.painter`, `ui.separator`, `ui.label`, `ui.button` etc. Se o componente não
  existe, **crie-o em `cardeal-ui` primeiro** (camada certa + galeria) e depois use. Antes de criar,
  procure o que já existe (`docs/12-ui-ux.md` §7.1). Rode `cargo xtask verificar-ui` antes de
  commitar UI. Regra completa: [ADR-0015](adr/0015-atomic-design-obrigatorio-na-ui.md). A dívida
  antiga foi zerada; a catraca (`xtask/ui-baseline.toml`, vazio) agora é "zero permitido".
- **Não monte uma listagem à mão, nem com componentes soltos.** Usar só `Grade` não basta: a barra de
  busca/filtro, o clique de ordenação e a coluna Editar/Excluir são padrões, e cada um virou
  componente (`BarraFiltros`, `Ordenacao`, `AcoesRegistro`) exatamente porque cinco telas os
  copiavam e divergiam (2026-09-21). Receita em `docs/12-ui-ux.md` §7.1; o xtask barra a cópia.
- Não relance subagentes Sonnet "grandes" sem necessidade clara — o histórico desta sessão
  teve quedas por limite de taxa; prefira trabalho direto e incremental.
- Não escreva código de um crate sem antes checar se `docs/contratos-internos.md` já fixa a
  assinatura — se fixar, implemente exatamente aquilo; se um crate exigir mudar a assinatura
  lá, atualize o documento no mesmo commit.
- Não commite sem `cargo check --workspace` limpo. Não existe exceção "é só um crate
  pequeno" — foi exatamente esse raciocínio que quebrou o SistemaX.
- Não assuma que o push para o GitHub já aconteceu — confira `git log origin/master` (ou
  peça para o usuário confirmar) antes de depender disso.
- Não invente um construtor que não existe (ex.: um fantasma de `Ctx::testavel(...)`) só
  para destravar um teste — antes de escrever o teste de uma função `pub fn` que roda fora
  do `Despachante`, confira se dá para montar um `Ctx` de verdade (`Ctx::de_sessao` — ver
  §1.2). Descobrir isso só na hora do teste de integração (como aconteceu com
  `mod-compras`) é sinal de que o design da função deveria ter checado isso mais cedo.
- CNPJ de teste inventado (dígitos "aleatórios") quase sempre falha o dígito verificador
  real — use `11222333000181` (já usado em vários testes) ou gere um novo com o algoritmo
  do módulo 11 antes de colar no teste; não adivinhe.
