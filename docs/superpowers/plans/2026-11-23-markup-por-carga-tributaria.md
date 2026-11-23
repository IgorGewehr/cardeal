# Markup pela carga tributária real — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** o preço sugerido de cada produto acerta a margem de lucro desejada depois de **todos** os impostos daquele produto, com a mesma conta que a nota emitida usa, e aparece pronto onde se digita preço — a começar pela peça na OS.

**Architecture:** um crate puro novo, `cardeal-tributos`, devolve a carga de um produto como termos lineares sobre o preço (por dentro, por fora, sobre o lucro). `mod-estoque::precificacao` resolve o preço em forma fechada sobre esses termos; `mod-fiscal::tributacao` emite a partir dos mesmos termos. A nota de compra alimenta o perfil (aprender) e o custo (crédito).

**Tech Stack:** Rust (workspace `cardeal`), SQLite (rusqlite), egui via `cardeal-ui`, `cardeal_kernel::{Dinheiro, Percentual, Preco}`.

**Spec:** `docs/superpowers/specs/2026-11-23-markup-por-carga-tributaria-design.md` — ler antes de cada tarefa.

## Global Constraints

- Aritmética exata: `Dinheiro`/`Percentual`/`Preco` e i128; nenhum `f64` em dinheiro; arredondamento ao centavo meio-acima.
- UI só com `cardeal-ui` (ADR-0015, `CLAUDE.md`); componente novo nasce em `cardeal-ui` com entrada em `examples/galeria.rs`; `cargo xtask verificar-ui` e `cargo xtask verificar-wasm` limpos.
- Cabe em 1280×720 lógicos.
- Nunca número inventado: dado faltando ⇒ `problema` em português dizendo o que preencher.
- Nunca sobrescrever em silêncio perfil fiscal ou preço definidos pelo usuário.
- Produto que hoje usa `RegimeImposto::Manual(%)` sem alíquota no perfil mantém exatamente o mesmo preço.
- Tabelas legais no código com fonte e data em comentário (LC 123/2006, LC 214/2025, Leis 10.147/2000, 10.485/2002, 13.097/2015, Conv. ICMS 142/2018) e o aviso "conferir com o contador".
- Edição de tributação exige `estoque.tributario.editar` (risco Alto); ver, `estoque.produto.ver`.
- Cada tarefa termina com `cargo test -p <crates tocados>`, `cargo clippy --workspace --all-targets -- -D warnings` e commit na `lunno` terminando com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Não commitar `SPED_EFD_*.txt` (dado da empresa, não rastreado de propósito).

## Review Focus

1. **Peça digitada que ainda não existe** (sem produto, sem custo médio): o markup precisa da categoria escolhida e do custo digitado — sem custo, nada de preço, só a dica. Teste na Tarefa 3.
2. **Usuário já digitou o preço e depois mexe no custo**: o preço dele não pode mudar. Teste na Tarefa 3.
3. **Soma de percentuais ≥ 100%** (imposto alto + margem alta): mensagem, não pânico nem preço negativo. Teste na Tarefa 5.
4. **Dados antigos com marca `icms_st` mas CST 102 no perfil** (as duas fontes discordando): vale o CST e o caso vai para "Tributação a conferir", sem mudar a nota emitida. Teste na Tarefa 6.
5. **Nota de compra estornada depois de gerar crédito e aprender o perfil**: o crédito sai do razão; o perfil aprendido fica (já está na lista para conferir). Teste na Tarefa 11.

---

## Marco 1 — a peça da OS já sai com preço (usa a carga de hoje)

### Task 1: Preço sugerido por categoria e custo, sem produto

**Files:**
- Modify: `crates/modulos/mod-estoque/src/consultas_preco.rs`
- Modify: `crates/modulos/mod-estoque/src/modulo.rs:118` (registro)
- Test: `crates/modulos/mod-estoque/tests/precificacao.rs`

**Interfaces:**
- Produces: `pub struct PrecoSugeridoNoGrupo { pub grupo: Id, pub custo: Dinheiro }`, `impl Consulta` com `Saida = SugestaoDePreco`, `PERMISSAO = "estoque.produto.ver"`, registrada como `"estoque.preco_sugerido_no_grupo.v1"`. `SugestaoDePreco.produto` vira `Option<Id>` e `nome` fica `""` quando não há produto (ajustar os usos).

- [ ] **Step 1: teste** `preco_no_grupo_usa_a_margem_e_a_comissao_herdadas`: grupo "Peças" com margem 15% e comissão 0, empresa Simples Anexo I com parâmetros fixos do teste (opex 10%, taxa 0); `PrecoSugeridoNoGrupo { grupo, custo: Dinheiro::reais(15) }` devolve `sugerido.preco` igual a `precificacao::preco_sugerido(Dinheiro::reais(15), &componentes_esperados)` e `origem_margem == OrigemMargem::Grupo(..)`. Segundo teste `preco_no_grupo_sem_margem_diz_o_que_falta`: grupo e empresa sem margem ⇒ `sugerido == None`, `problema` contém "margem".
- [ ] **Step 2:** `cargo test -p mod-estoque --test precificacao preco_no_grupo` → FAIL (tipo inexistente).
- [ ] **Step 3:** implementar reaproveitando `BaseDePreco::carregar` e a resolução de margem/comissão/perfil pelo grupo (a mesma que `linha_do_produto` usa para o grupo do produto).
- [ ] **Step 4:** rodar → PASS; `cargo test -p mod-estoque` inteiro verde.
- [ ] **Step 5:** commit `feat(estoque): preço sugerido por categoria e custo, sem produto`.

### Task 2: Peça rápida na categoria escolhida

**Files:**
- Modify: `crates/modulos/mod-estoque/src/comandos/criar_peca_rapida.rs`
- Test: `crates/modulos/mod-estoque/tests/comandos.rs`

**Interfaces:**
- Produces: `CriarPecaRapida { nome: String, #[serde(default)] grupo: Option<Id> }`. `None` mantém o grupo padrão "Peças" (código `PECAS`).

- [ ] **Step 1: teste** `peca_rapida_entra_na_categoria_informada` (grupo "Peças de celular" ⇒ produto criado nesse grupo) e `peca_rapida_sem_categoria_continua_em_pecas`.
- [ ] **Step 2:** rodar → FAIL.
- [ ] **Step 3:** implementar; grupo inexistente ou inativo ⇒ `Erro::nao_encontrado("grupo")`.
- [ ] **Step 4:** rodar → PASS.
- [ ] **Step 5:** commit `feat(estoque): peça rápida na categoria escolhida`.

### Task 3: A OS preenche o preço da peça pelo markup

**Files:**
- Modify: `crates/cardeal-desktop/src/tela_os/orcamento.rs` (`sugerir_preco`, `adicionar_peca`)
- Modify: `crates/cardeal-desktop/src/tela_os/itens.rs` (campo de categoria para peça nova; recálculo ao digitar custo)
- Modify: `crates/cardeal-desktop/src/tela_os/valores_peca.rs` (mesmo recálculo no diálogo Valores)
- Modify: `crates/cardeal-desktop/src/tela_os/mod.rs` (estado: `peca_grupo: Option<Id>`, `peca_preco_manual: bool`)
- Test: `crates/cardeal-desktop/src/tela_os/testes.rs`

**Interfaces:**
- Consumes: `estoque.preco_sugerido_no_grupo.v1` (Task 1), `estoque.preco_sugerido_do_produto.v1` (existente, com `custo: Some(..)`), `CriarPecaRapida.grupo` (Task 2).
- Produces: `fn escolher_sugestao(tabela: Option<(Preco, String)>, markup: Option<(Preco, String)>, ultimo: Option<Preco>) -> (Option<Preco>, String)` pura em `orcamento.rs` — ordem tabela > markup > último; o texto da dica é o segundo elemento.

- [ ] **Step 1: testes** em `testes.rs`:
  - `peca_do_estoque_sem_tabela_vem_com_o_preco_do_markup`: produto com custo médio R$ 15, grupo margem 15%, sem tabela e sem preço anterior ⇒ `peca_preco` = preço do markup e `peca_preco_dica` começa com `"Sugerido pela categoria"`.
  - `peca_nova_digitada_calcula_o_preco_ao_digitar_o_custo`: `peca_busca = "Tela A52"`, `peca_grupo = Some(pecas)`, `peca_custo = "15,00"`, chama a função de recálculo ⇒ `peca_preco` preenchido; `adicionar_peca` cria o produto no grupo `pecas`.
  - `preco_digitado_pelo_usuario_nao_muda_com_o_custo`: `peca_preco_manual = true`, `peca_preco = "40,00"`, muda o custo ⇒ continua `"40,00"`.
  - `tabela_de_preco_vence_o_markup`: com regra na tabela ⇒ preço da tabela.
  - teste unitário de `escolher_sugestao` cobrindo as quatro combinações.
  - o teste existente `peca_escolhida_sugere_quantidade_1_e_o_ultimo_preco_cobrado` continua passando (sem margem configurada no cenário ⇒ cai no último).
- [ ] **Step 2:** `cargo test -p cardeal-desktop tela_os` → FAIL.
- [ ] **Step 3:** implementar. A dica: `Sugerido pela categoria {grupo} · custo {custo} · lucro {margem}`. Campo "Categoria" (seletor em árvore já existente da aba Estrutura) só para peça nova, padrão o grupo `PECAS`. Digitar no campo de preço marca `peca_preco_manual = true`; `limpar_campos` zera.
- [ ] **Step 4:** testes → PASS; `cargo xtask verificar-ui` limpo.
- [ ] **Step 5:** commit `feat(os): preço da peça sugerido pelo markup da categoria`.

## Marco 2 — o motor de carga

### Task 4: Crate `cardeal-tributos` — a carga de um produto

**Files:**
- Create: `crates/cardeal-tributos/{Cargo.toml, src/lib.rs, src/simples.rs, src/normal.rs, src/termos.rs}`; adicionar ao workspace.
- Move: tabelas do Simples (`AnexoSimples`, `TETOS`, faixas, `reparticao_anexo_i`, `aliquota_efetiva`, `faixa_do_simples`) de `mod-estoque/src/precificacao.rs` para `cardeal-tributos/src/simples.rs`; `mod-estoque` reexporta (mesmos nomes públicos).
- Test: `crates/cardeal-tributos/tests/carga.rs`

**Interfaces (Produces):**
```rust
pub enum Regime { Mei, Simples { anexo: AnexoSimples, rbt12: Dinheiro }, Presumido { atividade: Atividade, receita_mensal: Dinheiro }, Real { pis_cofins_nao_cumulativo: bool }, Manual(Percentual) }
pub enum Atividade { Comercio, Industria, Servico }          // presunção IRPJ 8/8/32, CSLL 12/12/32
pub struct PerfilTributario { pub situacao_icms: Option<String>, pub aliquota_icms: Option<Percentual>, pub reducao_base: Option<Percentual>, pub aliquota_fcp: Option<Percentual>, pub cst_pis: Option<String>, pub cst_cofins: Option<String>, pub aliquota_pis: Option<Percentual>, pub aliquota_cofins: Option<Percentual>, pub aliquota_ipi: Option<Percentual>, pub ncm: Option<String> }
pub enum Natureza { PorDentro, PorFora, SobreOLucro }
pub enum Imposto { Das, Icms, Fcp, Pis, Cofins, Irpj, Csll, Ipi, IcmsSt, Cbs, Ibs, Seletivo, Manual }
pub struct Termo { pub imposto: Imposto, pub natureza: Natureza, pub coeficiente: Percentual, pub explicacao: String }
pub struct Carga { pub termos: Vec<Termo> }
impl Carga { pub fn por_dentro(&self) -> Percentual; pub fn por_fora(&self) -> Percentual; pub fn sobre_o_lucro(&self) -> Percentual }
pub fn e_substituido(p: &PerfilTributario) -> bool;   // CSOSN 500 ou CST 60
pub fn e_monofasico(p: &PerfilTributario) -> bool;    // CST PIS/COFINS 04
pub fn carga(regime: &Regime, perfil: &PerfilTributario, acima_do_sublimite: bool) -> Result<Carga, String>;
```
Valores fixados pela spec §5.1: ICMS `aliq × (1 − redução)` + FCP; PIS/COFINS `aliq × (1 − c_icms)`; Presumido comércio IRPJ+CSLL = 2,28% (8%×15% + 12%×9%), adicional 10% × 8% como coeficiente quando `receita_mensal × 8% > R$ 20.000`; Real `SobreOLucro` 34%; Simples Anexos I e II tirando ICMS só se substituído e PIS/COFINS se monofásico; CSOSN 201/202 não tira ICMS; acima do sublimite: DAS sem a parcela de ICMS + ICMS do perfil; IPI `PorFora`; MEI sem termos; `Manual(p)` um termo `Manual`.

- [ ] **Step 1: testes** (cada um com o número esperado calculado à mão no comentário):
  - `simples_anexo_i_st_paga_menos_que_revenda_comum` (critério 1 da spec, RBT12 R$ 500 mil);
  - `presumido_pis_cofins_nao_incide_sobre_o_icms` (ICMS 17% ⇒ PIS 0,65% × 0,83, COFINS 3% × 0,83);
  - `presumido_inclui_irpj_csll_de_2_28` e `presumido_liga_o_adicional_acima_de_20_mil_de_lucro_presumido`;
  - `real_tem_ir_sobre_o_lucro_de_34`;
  - `industria_substituta_no_simples_nao_tira_o_icms` (CSOSN 201);
  - `anexo_ii_monofasico_tira_pis_cofins`;
  - `acima_do_sublimite_icms_sai_do_das_e_entra_pelo_perfil` (critério 7);
  - `ipi_vai_por_fora`; `mei_nao_tem_termo`; `cst_normal_no_simples_e_erro_em_portugues`.
- [ ] **Step 2:** `cargo test -p cardeal-tributos` → FAIL.
- [ ] **Step 3:** implementar; repartição do Anexo II carregada da LC 123 Anexo II (fonte em comentário).
- [ ] **Step 4:** `cargo test -p cardeal-tributos` e `cargo test -p mod-estoque` (reexport) → PASS.
- [ ] **Step 5:** commit `feat(tributos): motor de carga por produto`.

### Task 5: Preço em forma fechada sobre a carga

**Files:**
- Create: `crates/cardeal-tributos/src/preco.rs`
- Test: `crates/cardeal-tributos/tests/preco.rs`

**Interfaces (Produces):**
```rust
pub struct Despesas { pub operacional: Percentual, pub comissao: Percentual, pub taxa_pagamento: Percentual }
pub struct ParcelaDoPreco { pub rotulo: String, pub valor: Dinheiro, pub explicacao: String }
pub struct PrecoResolvido { pub custo: Dinheiro, pub preco: Dinheiro, pub cobrado: Dinheiro, pub parcelas: Vec<ParcelaDoPreco>, pub lucro: Dinheiro, pub empate: Dinheiro }
pub fn resolver_preco(custo_liquido: Dinheiro, carga: &Carga, despesas: &Despesas, margem: Percentual) -> Result<PrecoResolvido, String>;
```
Fórmula da spec §5.1: `m' = m ÷ (1 − sobre_o_lucro)`; `divisor = 1 − (por_dentro + despesas + m')`; `preco = custo ÷ divisor`; `cobrado = preco × (1 + por_fora)`. `lucro` é o resto (absorve arredondamento). `custo + Σ parcelas + lucro == preco` sempre.

- [ ] **Step 1: testes** `mesmo_custo_mesma_margem_lucro_igual_com_cargas_diferentes` (dois perfis da Task 4; `lucro ÷ preco` = 15% ± 1 centavo e preços diferentes); `soma_100_por_cento_vira_mensagem` (Review Focus 3); `decomposicao_fecha_no_centavo` (propriedade sobre 1.000 combinações determinísticas de custo/percentuais); `real_lucro_depois_do_ir_e_a_margem_pedida`; `ipi_soma_ao_cobrado_e_nao_ao_preco`.
- [ ] **Step 2:** rodar → FAIL. **Step 3:** implementar. **Step 4:** PASS.
- [ ] **Step 5:** commit `feat(tributos): preço resolvido sobre a carga`.

### Task 6: Uma fonte para ST/monofásico, FCP no perfil

**Files:**
- Modify: `crates/modulos/mod-estoque/src/fiscal.rs` (`CamposFiscais.aliquota_fcp: Option<Percentual>`, `CamposFiscais::perfil_tributario(&self, ncm: Option<&str>) -> cardeal_tributos::PerfilTributario`)
- Modify: `crates/modulos/mod-estoque/src/migracoes.rs` (nova versão: coluna `aliquota_fcp` em grupo e produto; conversão das marcas; tabela `estoque_tributacao_a_conferir(id, empresa, produto, grupo, motivo, origem, antes BLOB, depois BLOB, criado_em, resolvido_em)`)
- Modify: `crates/modulos/mod-estoque/src/repositorio_preco.rs`, `consultas.rs:237` (marcas passam a ser derivadas do CST via `e_substituido`/`e_monofasico`)
- Test: `crates/modulos/mod-estoque/tests/precificacao.rs`

**Interfaces:**
- Produces: `fn registrar_a_conferir(uow, empresa: Id, alvo: AlvoTributacao, motivo: &str, origem: &str, antes: &CamposFiscais, depois: &CamposFiscais) -> Resultado<Id>` em `mod-estoque` (pública, usada pelas Tasks 10 e 12); `enum AlvoTributacao { Produto(Id), Grupo(Id) }`.

- [ ] **Step 1: testes** `migracao_grava_csosn_500_onde_so_havia_a_marca_st`; `marca_e_cst_discordando_vale_o_cst_e_vai_para_conferir` (Review Focus 4); `fcp_herda_do_grupo`.
- [ ] **Step 2–4:** FAIL → implementar → PASS (`cargo test -p mod-estoque -p mod-fiscal`).
- [ ] **Step 5:** commit `refactor(estoque): CST como fonte única de ST e monofásico; FCP no perfil`.

### Task 7: O preço sugerido usa o motor

**Files:**
- Modify: `crates/modulos/mod-estoque/src/precificacao.rs` (`carga_sobre_venda`/`carga_do_produto` viram adaptadores de `cardeal_tributos::carga`)
- Modify: `crates/modulos/mod-estoque/src/parametros_preco.rs` (regime do cadastro `nucleo_empresa.regime` → `Regime`; `pis_cofins_nao_cumulativo: Option<bool>`, `atividade: Atividade`)
- Modify: `crates/modulos/mod-estoque/src/consultas_preco.rs` (`SugestaoDePreco.resolvido: Option<cardeal_tributos::PrecoResolvido>`; `imposto` = soma por dentro, mantido)
- Test: `crates/modulos/mod-estoque/tests/precificacao.rs`

**Interfaces:**
- Consumes: Tasks 4–6. Produces: `SugestaoDePreco.resolvido` (lido pelas Tasks 3, 12, 13).

- [ ] **Step 1: testes** `criterio_1_simples_st_e_comum_mesmo_lucro`; `criterio_2_presumido_icms_17`; `manual_sem_aliquota_no_perfil_mantem_o_preco_de_antes` (compara com o número que a versão atual dá — gravar o valor antes de mexer); `regime_vem_do_cadastro_da_empresa`. Todos os testes existentes de `precificacao` continuam verdes.
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5:** commit `feat(estoque): preço sugerido pela carga real do produto`.

### Task 8: A emissão usa o motor (o preço prevê o que a nota cobra)

**Files:**
- Modify: `crates/modulos/mod-fiscal/src/tributacao.rs` (`tributar` obtém situação, base e alíquotas de `cardeal_tributos`; destaca FCP)
- Modify: `crates/cardeal-fiscal/src/...` (campos `pFCP`/`vFCP` no `IcmsItem` e no XML, se ainda não existirem; validar no XSD com os fixtures de `tests/dados/nfe_xsd`)
- Test: `crates/modulos/mod-fiscal/tests/fiscal.rs`

- [ ] **Step 1: testes** `imposto_emitido_e_o_previsto_para_todo_perfil` — para cada perfil suportado (CSOSN 102/103/300/400/500, CST 00/20/40/41/60, PIS/COFINS 01/04/49) e preços {1,00; 15,00; 999,99}, a soma dos impostos por dentro do item emitido == `Σ coeficientes × preço` arredondado (critério 6); `fcp_destacado_passa_no_xsd`. Testes de `tributacao` existentes intactos.
- [ ] **Step 2–4:** FAIL → implementar → PASS (`cargo test -p mod-fiscal -p cardeal-fiscal`).
- [ ] **Step 5:** commit `refactor(fiscal): emissão pelo mesmo motor do preço; FCP`.

## Marco 3 — simples para o dono

### Task 9: "Como este produto é tributado"

**Files:**
- Create: `crates/modulos/mod-estoque/src/tributacao_simples.rs` (opções prontas)
- Modify: aba Estrutura (detalhe do grupo) e ficha do produto em `crates/cardeal-desktop/src/` (telas de estoque)
- Test: `crates/modulos/mod-estoque/tests/precificacao.rs`, testes da tela

**Interfaces (Produces):**
```rust
pub enum OpcaoTributacao { RevendaComum, RevendaComSt, Monofasico, StEMonofasico, IsentoOuNaoTributado, Avancado }
pub fn campos_da_opcao(opcao: OpcaoTributacao, simples: bool, aliquota_icms_confirmada: Option<Percentual>) -> CamposFiscais;
pub fn opcao_dos_campos(c: &CamposFiscais, simples: bool) -> OpcaoTributacao;
pub fn aliquota_interna(uf: &str) -> Option<Percentual>;   // tabela por UF, com fonte
```
Mapeamento exato da tabela da spec §5.2. No regime normal, `RevendaComum` sem alíquota confirmada deixa `aliquota_icms = None` (o preço diz o que falta). Grava pelos comandos existentes `definir_campos_fiscais_do_grupo`/`_do_produto`.

- [ ] **Step 1: testes** de ida e volta (`opcao_dos_campos(campos_da_opcao(o)) == o` para todas as opções e os dois regimes); `revenda_comum_no_normal_nao_grava_aliquota_sem_confirmar`; tela: escolher "Revenda com ICMS já pago pelo fornecedor" no grupo grava CSOSN 500 e o produto herda.
- [ ] **Step 2–4:** FAIL → implementar → PASS; `cargo xtask verificar-ui`.
- [ ] **Step 5:** commit `feat(estoque): tributação da categoria em palavras do dono`.

### Task 10: A nota de compra ensina a tributação

**Files:**
- Modify: `crates/cardeal-fiscal/src/nfe.rs` (ler CST/valores de PIS e COFINS, CEST e `orig` por item)
- Modify: `crates/modulos/mod-compras/src/migracoes.rs` (colunas `situacao_icms`, `origem`, `valor_icms`, `valor_credito_sn`, `cst_pis`, `cst_cofins`, `valor_pis`, `valor_cofins`, `cest`)
- Create: `crates/modulos/mod-compras/src/aprender_tributacao.rs`
- Create: `crates/cardeal-tributos/src/ncm.rs` (`pub fn ncm_monofasico(ncm: &str) -> bool`, tabela com fonte)
- Test: `crates/modulos/mod-compras/tests/importacao.rs`

**Interfaces:**
- Consumes: `registrar_a_conferir` (Task 6). Produces: `fn aprender_do_item(uow, empresa: Id, produto: Id, item: &ItemNotaEntrada) -> Resultado<Option<Id>>` chamada na confirmação da entrada (manual e automática).

Regras (spec §5.3): só produto **sem perfil efetivo** (nem próprio nem herdado); CSOSN 500/CST 60/ST cobrado ⇒ substituído; CST PIS/COFINS 04 ou `ncm_monofasico` ⇒ monofásico; CEST e origem completam se vazios; perfil existente que a nota contradiz ⇒ só `registrar_a_conferir`, sem gravar.

- [ ] **Step 1: testes** `nota_com_csosn_500_ensina_st_ao_produto_sem_perfil` (critério 4, com desfazer voltando ao anterior); `produto_com_perfil_nunca_e_sobrescrito`; `nota_que_contradiz_o_perfil_vira_aviso`; `cest_e_origem_completam_o_produto`; parser: `le_cst_pis_cofins_e_cest` com XML de fixture.
- [ ] **Step 2–4:** FAIL → implementar → PASS (`cargo test -p mod-compras -p cardeal-fiscal`).
- [ ] **Step 5:** commit `feat(compras): a nota de compra ensina a tributação do produto`.

### Task 11: Crédito da compra no regime normal

**Files:**
- Modify: `crates/modulos/mod-compras/src/comandos/mod.rs` (`confirmar_entrada_comum`, `confirmar_entrada_com_parcelas`), `escrituracao.rs`, receituário em `docs/modulos/compras.md` §7
- Modify: plano de contas (contas `ICMS a recuperar`, `PIS a recuperar`, `COFINS a recuperar` — onde as contas padrão são semeadas)
- Test: `crates/modulos/mod-compras/tests/credito.rs` (novo)

**Interfaces:**
- Produces: `pub fn credito_do_item(regime: &Regime, perfil_saida: &PerfilTributario, item: &ItemNotaEntrada) -> CreditoDoItem { icms: Dinheiro, pis: Dinheiro, cofins: Dinheiro }` em `cardeal-tributos` (puro; regras §5.3: só Presumido/Real; ICMS só se a saída é tributada CST 00/20; PIS/COFINS só não cumulativo e não monofásico).

- [ ] **Step 1: testes** `regime_normal_entra_sem_o_icms_e_lanca_a_recuperar` (critério 3: custo do movimento = custo − crédito; razão com `D ICMS a recuperar`); `simples_nao_credita`; `revenda_com_st_nao_credita_icms`; `estorno_desfaz_o_credito` (Review Focus 5); `custo_medio_antigo_nao_e_recalculado`.
- [ ] **Step 2–4:** FAIL → implementar → PASS (`cargo test -p mod-compras -p mod-estoque -p cardeal-ledger`).
- [ ] **Step 5:** commit `feat(compras): crédito de ICMS/PIS/COFINS na entrada do regime normal`.

### Task 12: Para onde vai cada real, e a lista para conferir

**Files:**
- Create: componente `DecomposicaoDePreco` em `crates/cardeal-ui` (+ `examples/galeria.rs`)
- Modify: aba Preços do estoque e a dica da peça na OS (clique abre o painel)
- Modify: item do pedido de venda e do orçamento (`tela_vendas`, `tela_orcamentos`) e produto sem preço no PDV (`tela_pdv/estado.rs`): mesma sugestão e dica da Task 3, pela `escolher_sugestao` movida para um módulo compartilhado do desktop; no PDV só sugere, respeitando "preço sugerido obrigatório"
- Create: consulta `estoque.tributacao_a_conferir.v1` + comandos `estoque.resolver_tributacao_a_conferir.v1` (confirmar) e `estoque.desfazer_tributacao_a_conferir.v1`; tela em lista (padrão `FaixaKpi` → `BarraFiltros` → `Grade.com_acoes()`)
- Test: mod-estoque e testes da tela

**Interfaces:** Consumes `SugestaoDePreco.resolvido` (Task 7), tabela da Task 6.

- [ ] **Step 1: testes** `desfazer_volta_o_perfil_de_antes`; `confirmar_tira_da_lista`; tela: o painel mostra custo, cada imposto com a frase, operação, cartão, comissão e lucro e a soma fecha no preço.
- [ ] **Step 2–4:** FAIL → implementar → PASS; `verificar-ui` e `verificar-wasm`.
- [ ] **Step 5:** commit `feat(estoque): para onde vai cada real e tributação a conferir`.

### Task 13: Impostos por produto — previsto e realizado

**Files:**
- Create: `crates/modulos/mod-estoque/src/consultas_impostos.rs` (`estoque.impostos_por_produto.v1 { de: Data, ate: Data, grupo: Option<Id> }`)
- Modify: Estoque › Análise (nova aba), exportação PDF/xlsx como os demais relatórios

**Interfaces (Produces):** linha `{ produto, grupo, previsto_por_unidade: Dinheiro, credito_por_unidade: Dinheiro, vendido: Quantidade, imposto_realizado: Dinheiro, credito_realizado: Dinheiro }`; realizado = soma dos itens das notas de venda autorizadas (mod-fiscal) e de entrada confirmadas (mod-compras) no período, lidos pelas consultas públicas desses módulos (não pelas tabelas).

- [ ] **Step 1: testes** com uma NFC-e emitida no simulado e uma entrada: os totais batem com as notas.
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5:** commit `feat(estoque): relatório de impostos por produto`.

## Marco 4 — destino e reforma (simulação)

### Task 14: Simular outro destino

**Files:** `crates/cardeal-tributos/src/destino.rs`; parâmetro `destino: Option<Destino>` em `PrecoSugeridoDoProduto`; seletor no painel da Task 12.

**Interfaces:** `pub enum Destino { InternoConsumidor, Interestadual { uf: String, contribuinte: bool } }`; `pub fn aliquota_interestadual(origem_uf: &str, destino_uf: &str, origem_mercadoria: u8) -> Percentual` (4% importado origens 1,2,3,8; 7% Sul/Sudeste→N/NE/CO/ES; 12% demais); DIFAL ao não contribuinte = interna do destino − interestadual (sobre base dupla, por dentro).

- [ ] Testes `importado_vai_a_4`, `sc_para_ba_e_7`, `difal_ao_consumidor_de_outra_uf`; FAIL → implementar → PASS; commit `feat(tributos): simulação por destino`.

### Task 15: Reforma tributária — projeção por ano

**Files:** `crates/cardeal-tributos/src/reforma.rs`; consulta `estoque.projecao_da_reforma.v1 { produto }`; painel "Daqui a alguns anos" (2026, 2027, 2029, 2033) na Task 12.

**Interfaces:** `pub fn carga_no_ano(regime: &Regime, perfil: &PerfilTributario, ano: u16, simples_ibs_cbs_por_fora: bool) -> Carga` — cronograma da spec §5.7; CBS/IBS `PorFora` com coeficiente `aliq × (1 − c_icms)` na transição; IS por NCM (bebidas cap. 22, tabaco cap. 24) a partir de 2027; alíquotas de referência estimadas como constantes marcadas `ESTIMADA` com a fonte.

- [ ] Testes `2026_teste_nao_custa_ao_regime_normal`, `2027_sem_pis_cofins`, `cbs_na_transicao_exclui_o_icms_da_base`, `simples_fora_do_ibs_cbs_por_padrao`; FAIL → implementar → PASS; commit `feat(tributos): projeção da reforma tributária`.

### Task 16: Documentação

**Files:** `docs/adr/0022-markup-pela-carga-tributaria-real.md` (+ índice), `docs/modulos/estoque.md`, `compras.md`, `os.md`, `fiscal.md`, `docs/19-estado-e-processo.md` (entrada da sessão com o que foi e o que não foi verificado).

- [ ] Escrever; `cargo test --workspace`, clippy, `verificar-ui`, `verificar-wasm` verdes; commit `docs: ADR-0022 e módulos`; push.
