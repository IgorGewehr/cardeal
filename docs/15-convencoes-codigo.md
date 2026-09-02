# 15 — Convenções de código

> Estas convenções existem para que qualquer pessoa consiga abrir qualquer arquivo do projeto e
> reconhecer imediatamente onde está e o que esperar. Elas são verificadas por lint e revisão.

## 1. Idioma

**Domínio em português. Infraestrutura em inglês idiomático de Rust.**

```rust
// ✅ Domínio — o vocabulário do negócio brasileiro
pub struct Lancamento { pub competencia: Data, pub partidas: Vec<Partida> }
pub enum EstadoParcela { Aberta, Parcial, Quitada, Cancelada }
pub fn baixar_parcela(&self, parcela: Id, valor: Dinheiro) -> Resultado<Baixa>;

// ✅ Infraestrutura — Rust é inglês
impl Display for Dinheiro { fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { ... } }
impl Iterator for Partidas { type Item = Partida; fn next(&mut self) -> Option<Self::Item> { ... } }
```

Por quê: a equipe pensa e conversa com o cliente em português. Traduzir "competência" para
`accrual_date` cria uma camada de tradução mental em toda conversa e toda revisão. E `competencia`
não tem tradução limpa mesmo — é um conceito do direito contábil brasileiro.

Regras práticas:

- Sem acentos em identificadores (`competencia`, não `competência`); **com** acentos em toda string
  visível ao usuário e em toda documentação.
- Traits de biblioteca padrão, tipos genéricos (`T`, `E`), e nomes exigidos pelo ecossistema
  permanecem em inglês.
- `Resultado<T> = std::result::Result<T, Erro>` é o alias do projeto. Use sempre.
- Comentários e `///` em português.
- Nomes de tabela e coluna em português, sem acento (ver [doc 06](06-modelo-de-dados.md)).

## 2. Estrutura de um módulo

```
mod-<nome>/
├── Cargo.toml
├── README.md              ← especificação funcional; escrita ANTES do código
├── migracoes/
│   └── NNNN_descricao.sql
├── src/
│   ├── lib.rs             ← impl Modulo; re-exports públicos. Sem lógica.
│   ├── manifesto.rs       ← Manifesto, permissões, menu, contas requeridas
│   ├── dominio/           ← SEM I/O. Tipos, regras, invariantes. Testável sem banco.
│   │   ├── mod.rs
│   │   └── <agregado>.rs
│   ├── comandos/          ← um arquivo por comando
│   │   ├── mod.rs
│   │   └── <verbo_substantivo>.rs
│   ├── consultas/         ← um arquivo por consulta
│   ├── repositorio.rs     ← todo o SQL do módulo. Único lugar com strings SQL.
│   ├── eventos.rs         ← eventos publicados e assinados
│   ├── erros.rs           ← enum de erro do módulo
│   └── telas/             ← UI (feature "ui")
└── testes/
    ├── integracao.rs
    └── receituario.rs     ← prova a linha do módulo na tabela do doc 05
```

**A separação `dominio/` sem I/O é obrigatória e verificada.** `cargo xtask arquitetura` falha se
um arquivo em `dominio/` importar `rusqlite`, `tokio`, `reqwest` ou o repositório.

## 3. Comandos

Um arquivo, um comando. Estrutura fixa:

```rust
//! Baixa (liquida) uma parcela de título a receber ou a pagar.
//!
//! Receituário: D Caixa/Bancos · C Clientes a receber   (ver docs/05 §5)

use crate::prelude::*;

#[comando(permissao = "financeiro.receber.baixar", risco = Medio, audita)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaixarParcela {
    pub parcela: Id,
    pub valor: Dinheiro,
    pub data: Data,
    pub conta_destino: Id,
    pub juros: Option<Dinheiro>,
    pub desconto: Option<Dinheiro>,
    pub observacao: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BaixaRealizada {
    pub baixa: Id,
    pub lancamento: Id,
    pub saldo_restante: Dinheiro,
    pub parcela_quitada: bool,
}

impl Comando for BaixarParcela {
    type Saida = BaixaRealizada;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar
        let parcela = repo::parcela(uow, self.parcela)?
            .ok_or(ErroFinanceiro::ParcelaNaoEncontrada(self.parcela))?;

        // 2. Validar (regras puras — moram em dominio/)
        let baixa = parcela.calcular_baixa(self.valor, self.data, self.juros, self.desconto)?;

        // 3. Efeito no razão
        let lanc = ConstrutorLancamento::novo(ctx.empresa, self.data, baixa.historico())
            .origem(Origem::modulo("financeiro").agregado(parcela.id))
            .liquidacao(self.data)
            .debitar(self.conta_destino, baixa.valor_recebido)
            .creditar(parcela.conta_contraparte, baixa.principal)
            .creditar_se(ctx.contas.receita_financeira(), baixa.juros)
            .debitar_se(ctx.contas.descontos_concedidos(), baixa.desconto)
            .construir()?;
        let lancamento = ctx.razao().registrar(uow, lanc)?;

        // 4. Persistir
        repo::gravar_baixa(uow, &baixa, lancamento)?;

        // 5. Publicar
        uow.publicar(eventos::ParcelaBaixada {
            parcela: parcela.id, valor: baixa.valor_recebido, quitada: baixa.quita(),
        })?;

        Ok(BaixaRealizada { /* ... */ })
    }
}
```

As cinco etapas — **carregar, validar, lançar no razão, persistir, publicar** — aparecem nesta ordem
em todo comando do sistema. Quem leu um, leu todos.

## 4. Erros

Um enum por módulo, com `thiserror`. Toda variante carrega o suficiente para a mensagem ao usuário.

```rust
#[derive(Debug, thiserror::Error)]
pub enum ErroFinanceiro {
    #[error("Parcela não encontrada")]
    ParcelaNaoEncontrada(Id),

    #[error("A parcela já foi quitada em {data}")]
    ParcelaJaQuitada { data: Data },

    #[error("Valor da baixa (R$ {informado}) supera o saldo devedor (R$ {saldo})")]
    ValorSuperaSaldo { informado: Dinheiro, saldo: Dinheiro },

    #[error("O caixa está fechado desde {fechado_em}")]
    CaixaFechado { caixa: Id, fechado_em: Instante },
}

impl ErroDominio for ErroFinanceiro {
    fn codigo(&self) -> CodigoErro { /* mapeia para o catálogo do doc 09 */ }
    fn detalhes(&self) -> Detalhes { /* resumo, causa e ações sugeridas em português */ }
}
```

Proibido: `anyhow` em biblioteca (só em `xtask` e binários), `Box<dyn Error>` em API pública,
`String` como erro, `unwrap()`/`expect()` fora de teste sem comentário `// SEGURO:`.

## 5. SQL

Todo SQL vive em `repositorio.rs`. Sempre parâmetros nomeados. Sempre colunas explícitas.

```rust
pub fn parcelas_vencendo(
    c: &Connection, empresa: Id, de: Data, ate: Data, cursor: Option<Cursor>,
) -> Resultado<Pagina<ParcelaResumo>> {
    const SQL: &str = "
        SELECT p.id, p.vencimento, p.valor, p.valor_baixado, p.estado,
               t.contraparte_id, c.nome
        FROM   financeiro_parcela p
        JOIN   financeiro_titulo  t ON t.id = p.titulo
        LEFT   JOIN cliente c ON c.id = t.contraparte_id
        WHERE  t.empresa = :empresa
          AND  p.estado IN ('Aberta','Parcial')
          AND  p.vencimento BETWEEN :de AND :ate
          AND  (:cursor IS NULL OR (p.vencimento, p.id) > (:cur_venc, :cur_id))
        ORDER  BY p.vencimento, p.id
        LIMIT  :limite";
    // ...
}
```

Regras: `const SQL` no topo da função (fácil de achar e de copiar para o `EXPLAIN`), nunca
concatenação, nunca `SELECT *`, nunca `OFFSET`, sempre `LIMIT`.

## 6. Nomes

| Coisa | Convenção | Exemplo |
|---|---|---|
| Comando | `VerboSubstantivo` | `BaixarParcela`, `FinalizarVenda`, `AbrirCaixa` |
| Saída de comando | `SubstantivoParticipio` | `BaixaRealizada`, `VendaFinalizada` |
| Consulta | `SubstantivoContexto` | `ParcelasVencendo`, `SaldoPorConta` |
| Evento | `<modulo>.<substantivo>_<participio>.v<N>` | `financeiro.parcela_baixada.v1` |
| Permissão | `<modulo>.<recurso>.<acao>` | `financeiro.receber.baixar` |
| Tabela | `<modulo>_<entidade>` | `financeiro_parcela` |
| Erro | `Erro<Modulo>` | `ErroFinanceiro` |
| Trait de porta | `Porta<Coisa>` | `PortaFiscal`, `PortaImpressora` |
| Feature de submódulo | `snake_case` do id | `conciliacao`, `centro_custo` |

## 7. Estilo

- `rustfmt` padrão, largura **100**.
- `clippy::pedantic` ligado, com exceções justificadas por `#[allow]` **e comentário**.
- `#![forbid(unsafe_code)]` em todo crate sem FFI.
- Sem `mod.rs` gigante: se passa de 300 linhas, divida.
- Sem função com mais de ~60 linhas ou 3 níveis de aninhamento. Extraia.
- Sem `impl` com mais de ~12 métodos públicos. Provavelmente são dois tipos.
- Docstring `///` em **todo item público**, começando por verbo no infinitivo.
- Constantes de negócio nomeadas, nunca literais soltos:
  `const TOLERANCIA_QUEBRA_CAIXA: Dinheiro = Dinheiro::centavos(500);`

## 8. Dinheiro — as regras inegociáveis

1. **Nunca** `f32`/`f64` para valor monetário. Nem em cálculo intermediário. Nem "só para exibir".
2. `Dinheiro` é `i64` em centavos. Overflow é checado (`checked_add`), não silencioso.
3. Percentual é tipo próprio (`Percentual`, base 1e-6), não `f64`.
4. Quantidade é `Quantidade` (escala 1e-4), preço unitário é `Preco` (escala 1e-6) — a NF-e permite
   até 4 e 10 casas respectivamente, e arredondar cedo gera divergência de centavos na nota.
5. Arredondamento é **explícito**: `Dinheiro::de_preco(qtd, preco, Arredondamento::MeioAcima)`.
   Não existe conversão implícita.
6. Ratear um total em parcelas usa `Dinheiro::ratear(n)`, que distribui os centavos que sobram —
   e é testado por propriedade.
7. Comparar dinheiro com `==` é seguro (é inteiro). Comparar percentual em cálculo, não.

## 9. Concorrência

- Domínio é síncrono. `async` só na borda ([doc 02 §2.5](02-pilar-eficiencia.md)).
- Escrita só pelo escritor único. Nenhum `Connection` mutável circula pelo código de módulo.
- Estado compartilhado: `Arc<T>` imutável, ou `parking_lot::RwLock` com escopo mínimo.
  Nunca segure um lock através de `await` ou de I/O.
- Canais: `crossbeam` para o escritor, `tokio::sync` na borda async.

## 10. Antes de abrir o PR

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask arquitetura
cargo xtask orcamento
```

E no corpo do PR: qual seção da documentação isso implementa, e o `EXPLAIN QUERY PLAN` de toda
consulta nova.
