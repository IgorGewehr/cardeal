# 19 — Estado atual e processo de trabalho

> **Leia este documento primeiro** — antes do onboarding (doc 16), antes do roadmap (doc 17).
> Eles descrevem o projeto *terminado* e a *intenção*; este descreve **o que existe agora**,
> **como chegamos até aqui** e **onde continuar**. É o documento que qualquer IA ou pessoa
> nova precisa ler para retomar o trabalho sem repetir decisões já tomadas ou quebrar o que
> já funciona.
>
> Mantenha-o atualizado a cada sessão de trabalho relevante. Se ele mentir sobre o estado
> real do repositório, ele é pior que não existir.

## 1. O que existe, com precisão, hoje (2026-09-05)

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
     validade — o `gestao-raiz` reconverte para 3DES no upload porque libs de assinatura
     antigas não leem PFX moderno em AES-256; **confirmar se o `sefaz-api` do usuário
     precisa do mesmo** antes de implementar), `PortaFiscal`/`ApiFiscalHttp` mirado no
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
