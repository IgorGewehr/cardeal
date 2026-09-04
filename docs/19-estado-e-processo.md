# 19 — Estado atual e processo de trabalho

> **Leia este documento primeiro** — antes do onboarding (doc 16), antes do roadmap (doc 17).
> Eles descrevem o projeto *terminado* e a *intenção*; este descreve **o que existe agora**,
> **como chegamos até aqui** e **onde continuar**. É o documento que qualquer IA ou pessoa
> nova precisa ler para retomar o trabalho sem repetir decisões já tomadas ou quebrar o que
> já funciona.
>
> Mantenha-o atualizado a cada sessão de trabalho relevante. Se ele mentir sobre o estado
> real do repositório, ele é pior que não existir.

## 1. O que existe, com precisão, hoje (2026-09-03)

Todo o **workspace compila** (`cargo check --workspace`) e está formatado
(`cargo fmt --check`) neste exato commit. Não existe estado "quebrado" no HEAD — essa é uma
garantia deliberada deste projeto (ver §4 e §7 sobre por quê isso importa tanto).

### 1.1 Crates com domínio real, implementado e testado

| Crate | O que tem | Testes |
|---|---|---|
| `cardeal-kernel` | `Dinheiro`, `Quantidade`, `Preco`, `Percentual` (aritmética exata, sem ponto flutuante), `Id`/`ChaveIdempotencia`/`Versao` (UUIDv7), `Data`/`Instante`/`Competencia`/`Periodo`/`Fuso`, `Cpf`/`Cnpj`/`Uf`/`InscricaoEstadual` (com dígito verificador real), `Erro`/`Detalhes`/`ErroDominio`, utilidades de texto (busca, similaridade, redação de PII) | **44 unit + 7 doctest** |
| `cardeal-ledger` | O Razão: `Conta`/`CodigoConta`/`PapelConta`, `plano_padrao()`, `Lancamento`/`Partida`/`EstadoLancamento`, `ConstrutorLancamento` → `LancamentoBalanceado` (invariante débito=crédito provado no tipo), `Razao` (registrar/estornar/confirmar/liquidar) e `Contas` — genéricos sobre `PortaRazao`, com **duas** implementações: o double em memória e **`RepositorioRazao`** (adaptador SQLite real sobre a `UnidadeDeTrabalho`). `migracoes::conjunto()` traz as tabelas `razao_*` | **25 unit + 3 integração (SQLite real) + 2 doctest + 1 propriedade** |
| `cardeal-modkit` | Os tipos declarativos (`Manifesto`/`Submodulo`/`Permissao`/`EntradaMenu`/`ContaPadrao`) com `Manifesto::validar()`, o **`RegistroModulos`** (grafo de `depende_de`, ordem topológica, conjunto efetivo por empresa), **e o despacho**: traits `Modulo`/`Comando`/`Consulta`, `Ctx` de execução, `Registro` (declaração encadeável), `Ambiente` (conjunto efetivo por empresa) e `Despachante` — resolve módulo ativo + `cardeal_auth::autorizar` e roda o manipulador dentro da transação do `Escritor` (comando) ou sobre o `Leitor` (consulta); carga/saída em `postcard`; erro de domínio desfaz o `SAVEPOINT` e sobe intacto | **25 unit (7 de despacho end-to-end, SQLite real) + 1 doctest** |
| `cardeal-storage` | **Escritor único + group commit** (`docs/07`, ADR-0012): `Armazenamento` (abre, migra o núcleo, sobe o escritor e o pool de leitura), `Escritor::executar` (enfileira uma `UnidadeDeTrabalho`, `SAVEPOINT` por tarefa, um `COMMIT`/`fsync` por lote; tarefa que falha ou entra em pânico é isolada), `UnidadeDeTrabalho` (`conexao`, `publicar` → outbox na mesma transação, `auditar` → cadeia BLAKE3, `proximo_numero`/`reservar_faixa`, `travar`), `Leitor` (pool `query_only` sobre o snapshot do WAL), executor de migrações com ordenação topológica e hash imutável, `Segredo<T>`, esquema `nucleo_*` do doc 06 (migração `nucleo` v2 adiciona `nucleo_papel_limite`), `ErroArmazenamento::Dominio(Erro)` (`From<Erro>`) — erro de domínio dentro de uma tarefa desfaz o `SAVEPOINT` e sobe intacto | **9 unit (integração, WAL real)** |
| `cardeal-auth` | `hash_senha`/`verificar_senha` (Argon2id real, parâmetros do doc 08), `PoliticaSenha`, `Bloqueio` (progressivo 5→1min/10→15min), `Escopo` (ABAC — "gerente da filial 2 não vê o caixa da filial 1"), **`Usuario`** (autenticar/trocar/redefinir senha, ativar; erro genérico anti-enumeração), **`Papel`/`PapelDeFabrica`/`PoliticaPapel`** (os 9 papéis de §3.2 como regra declarativa expandida contra o catálogo real — sem depender de `cardeal-modkit`), **`ValorLimite`** (§3.4), **`Sessao`/`AutorizacoesEfetivas`** e a função livre **`autorizar(sessao, permissao, recurso)`** (concede a permissão **e** o escopo abrange o recurso). **`RepositorioAuth` + `consultas`**: adaptador SQLite de `nucleo_usuario`/`nucleo_papel`/`nucleo_papel_permissao`/`nucleo_papel_limite`/`nucleo_usuario_papel` sobre a `UnidadeDeTrabalho`, mesmo padrão do `RepositorioRazao` | **50 unit + 7 integração (SQLite real) + 2 doctest** |
| `mod-financeiro` (parcial) | Domínio puro: `ConstrutorTitulo` → `TituloComParcelas` (rateio que conserva o total ao centavo), `Parcela::situacao_em`/`planejar_baixa` (juros/multa/desconto **calculados na leitura**), `SessaoCaixa` (**fechamento cego** com apuração de quebra), `Recorrencia`, o `MANIFESTO` completo, o `receituario`. **E o primeiro módulo que atravessa o despacho de verdade**: `impl Modulo for ModuloFinanceiro` (manifesto + migrações `financeiro_titulo`/`financeiro_parcela`/`financeiro_baixa`), `RepositorioFinanceiro` (todo o SQL do módulo), os eventos, e os comandos **`LancarTituloAReceber`** e **`BaixarRecebimento`** — cada um nas 5 etapas do doc 15 §3, resolvendo `Contas` por papel e postando via `Razao::registrar` | **40 unit (incl. 3 propriedades) + 5 integração (Despachante + SQLite real) + 1 doctest** |
| `mod-clientes` (parcial) | Domínio puro: `Pessoa`/`Papel`/`ConstrutorPessoa` (papéis que coexistem, FSM `Ativa→Inativa→Anonimizada`, anonimização LGPD que preserva o `id`), `DocumentoPessoa`/`Endereco`/`Contato` (CPF/CNPJ por dígito verificador sem I/O, UF, CEP, formato de contato), `LimiteCredito`/`Score` (bloqueio automático, liberação manual e auditada, score derivado), `dedup` (sugestão por documento e por similaridade de nome), `MANIFESTO` validado | **23 unit + 1 propriedade** |
| `mod-estoque` (parcial) | Domínio puro: `Produto`/`Variacao`/`validar_gtin`/`Conversao`/`Lote` (NCM e GTIN por dígito verificador sem I/O, "validade exige lote"), `SaldoLocal`/`custo_medio_movel` (custo médio recalculado só na entrada, saldo negativo aceito e sinalizado, reserva que não é saída), `Inventario` (contagem cega + geração de ajustes), o `receituario` dos lançamentos que o próprio estoque posta (ajuste, perda, transferência, entrada avulsa), `MANIFESTO` validado | **20 unit + 1 propriedade** |
| `mod-vendas` (parcial) | Domínio puro: `preco_vigente` sobre `TabelaPreco`/`RegraPreco` (faixa de quantidade + promoção com vigência; a regra mais específica vence), `Orcamento`/`Pedido`/`ItemVenda` (as duas FSMs, total dos itens, preço congelado, faturar irreversível, desconto acima do teto do papel recusado na hora), `Devolucao` (total/parcial com rateio proporcional sem perder centavo), `Comissao` (base líquida de devolução), o `receituario` (faturamento com receita + desconto + CMV, devolução, comissão), `MANIFESTO` validado (depende de financeiro + clientes + estoque) | **26 unit + 1 propriedade** |

**Total: 274 testes (incl. 22 de integração SQLite) + 13 doctests + 7 propriedades. Tudo verde.**

### 1.2 Crates ainda como stub

Os outros 18 crates do workspace (`cardeal-protocol`, `cardeal-server`, `cardeal-cliente`,
`cardeal-ui`, `cardeal-analytics`, `cardeal-fiscal`, `cardeal-testkit`, `cardeal-desktop`,
`xtask` e os 9 `mod-*` restantes) têm só `Cargo.toml` válido + `src/lib.rs`/`src/main.rs`
mínimo apontando para este documento e para o roadmap. Isso é proposital: garante que
`cargo check --workspace` sempre passa, mesmo com a maior parte do sistema ainda não
escrita — ver §4.

`mod-clientes`, `mod-estoque` e `mod-vendas` já saíram do estado de stub, mas só na camada
de **domínio puro** (§1.1): tipos e transições testadas, sem banco. `cardeal-storage`,
`cardeal-ledger` (com `RepositorioRazao`), `cardeal-auth` (domínio de autorização +
`RepositorioAuth`) e `cardeal-modkit` (`RegistroModulos` **+ o despacho
`Modulo`/`Comando`/`Consulta`/`Ctx`/`Registro`/`Despachante`**) já existem e passam.

**`mod-financeiro` foi o primeiro módulo a atravessar o despacho de verdade**:
`impl Modulo`, migrações próprias, `RepositorioFinanceiro`, eventos, e os comandos
`LancarTituloAReceber`/`BaixarRecebimento` (§1.1), com 5 testes de integração que vão do
`Despachante` ao Razão e às tabelas `financeiro_*`. O que falta: **(a)** o resto dos
comandos do financeiro (espelho a pagar, caixa, estorno) e dos outros `mod-*`, na ordem do
roadmap; **(b)** a montagem de `AutorizacoesEfetivas` cruzando os papéis do usuário com o
`ConjuntoEfetivo` (em `cardeal-server`); **(c)** `Ctx::contas()` (espera um resolvedor
não-genérico em `cardeal-ledger`) — por ora o comando faz
`Contas::nova(&RepositorioRazao::novo(uow), empresa)`.
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
3. Próximo passo natural, na mesma disciplina de domínio-primeiro:
   - **`mod-financeiro`: completar os comandos** — o padrão está montado
     (`src/comandos/*.rs`, `src/repositorio.rs`, `src/eventos.rs`, `tests/comandos.rs`).
     Faltam: o **espelho a pagar** (`LancarTituloAPagar`/`BaixarPagamento` — permissão
     `financeiro.pagar.*`, contraparte `Fornecedor`, D/C invertidos no `receituario`, que já
     trata as duas espécies); os comandos de **caixa** (`CadastrarCaixa` — não está no §5
     do spec, precisa de decisão + doc; depois `AbrirCaixa`/`RegistrarSuprimento`/
     `RegistrarSangria`/`FecharCaixa`, todo o domínio `SessaoCaixa` já pronto); `EstornarBaixa`.
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
   (`../sefaz-api`) — não como emissor próprio. **Antes disso ir para produção**, o
   `sefaz-api` precisa da rotação de segredos e do endurecimento citados em §2.

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
