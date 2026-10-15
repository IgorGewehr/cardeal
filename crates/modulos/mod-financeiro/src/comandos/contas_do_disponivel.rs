//! As contas do Disponível (caixa, bancos) no dia a dia: o **saldo inicial** de cada uma (o
//! primeiro dia de qualquer cliente: "tenho R$ 12 mil no banco hoje"), a **transferência**
//! entre elas (depositar o dinheiro do caixa no banco, mover de um banco para outro) e
//! **desativar** a conta que não se usa mais.
//!
//! Lições do `gestao-raiz`, que já passou por isso com usuários reais:
//! - o saldo inicial é um só por conta — redefinir ajusta, não soma (lá havia duas fontes de
//!   saldo inicial e as telas divergiam);
//! - a transferência é um par atômico débito/crédito, e cancelar estorna os dois lados;
//! - não se desativa conta com saldo ("transfira antes"), nem a do caixa em uso.
//!
//! Aqui, no Razão:
//! - saldo inicial: D conta / C Lucros acumulados (3.2), `Realizado` na data da implantação;
//! - transferência: D destino / C origem;
//! - origem `financeiro/saldo_inicial` (por conta) e `financeiro/transferencia`.
//!
//! Dinheiro em espécie não fica negativo: transferir de um caixa exige saldo. Banco pode (o
//! cheque especial existe).

use cardeal_kernel::{Data, Dinheiro, Id, Instante};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::erros::ErroFinanceiro;
#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, data_de, id_de, persist};

/// Os papéis de conta que o sistema usa por conta própria (PDV, sangria, baixas): não se
/// desativam.
#[cfg(feature = "sqlite")]
const PAPEIS_DO_SISTEMA: &[&str] = &["Caixa", "Bancos", "Aplicacoes", "ValoresEmTransito"];

/// Uma conta do Disponível como o comando precisa vê-la.
#[cfg(feature = "sqlite")]
struct ContaDisponivel {
    nome: String,
    papel: Option<String>,
}

/// `(empresa, codigo, nome, tipo, ativa, papel_padrao)` de `razao_conta`.
#[cfg(feature = "sqlite")]
type LinhaConta = (Vec<u8>, String, String, String, i64, Option<String>);

/// Carrega e confere que `conta` é uma conta analítica ativa do Disponível (1.1.*) da empresa.
#[cfg(feature = "sqlite")]
fn conta_disponivel(c: &Connection, empresa: Id, conta: Id) -> Resultado<ContaDisponivel> {
    let linha: Option<LinhaConta> = c
        .query_row(
            "SELECT empresa, codigo, nome, tipo, ativa, papel_padrao FROM razao_conta WHERE id = ?1",
            [blob(conta)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()
        .map_err(persist)?;
    let Some((dona, codigo, nome, tipo, ativa, papel)) = linha else {
        return Err(Erro::nao_encontrado("conta"));
    };
    if id_de(dona) != empresa || tipo != "Analitica" || !codigo.starts_with("1.1.") {
        return Err(Erro::de_dominio(&ErroFinanceiro::ContaDisponivelInvalida(
            "A conta precisa ser um caixa ou banco (grupo 1.1 Disponível)",
        )));
    }
    if ativa == 0 {
        return Err(Erro::de_dominio(&ErroFinanceiro::ContaDisponivelInvalida(
            "A conta está desativada",
        )));
    }
    Ok(ContaDisponivel { nome, papel })
}

/// A conta é dinheiro em espécie: o papel Caixa ou a conta de um caixa cadastrado.
#[cfg(feature = "sqlite")]
fn e_caixa(c: &Connection, empresa: Id, conta: Id, papel: Option<&str>) -> Resultado<bool> {
    if papel == Some("Caixa") {
        return Ok(true);
    }
    c.query_row(
        "SELECT 1 FROM financeiro_caixa WHERE empresa = ?1 AND conta_razao = ?2 AND ativo = 1",
        params![blob(empresa), blob(conta)],
        |_| Ok(()),
    )
    .optional()
    .map(|o| o.is_some())
    .map_err(persist)
}

#[cfg(feature = "sqlite")]
fn saldo(uow: &mut UnidadeDeTrabalho, conta: Id) -> Resultado<Dinheiro> {
    cardeal_ledger::RepositorioRazao::novo(uow)
        .saldo_realizado(conta)
        .map_err(|e| Erro::de_dominio(&e))
}

// ── saldo inicial ────────────────────────────────────────────────────────────

/// Define o saldo de uma conta do Disponível na data da implantação. Redefinir **substitui** o
/// anterior (estorna e lança de novo), nunca soma; zero apaga o saldo inicial.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirSaldoInicial {
    /// A conta (caixa ou banco).
    pub conta: Id,
    /// O saldo naquele dia — negativo num banco no cheque especial.
    pub saldo: Dinheiro,
    /// A data da implantação.
    pub data: Data,
}

/// O que `DefinirSaldoInicial` devolve.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SaldoInicialDefinido {
    /// O lançamento do saldo inicial (`None` quando o saldo é zero).
    pub lancamento: Option<Id>,
    /// O saldo da conta depois disso.
    pub saldo_atual: Dinheiro,
}

impl Comando for DefinirSaldoInicial {
    type Saida = SaldoInicialDefinido;
    const PERMISSAO: &'static str = "financeiro.banco.editar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let conta = conta_disponivel(uow.conexao(), ctx.empresa, self.conta)?;

        // O saldo inicial é um só: o que havia é estornado antes.
        {
            let mut repo = cardeal_ledger::RepositorioRazao::novo(uow);
            let vivos = repo
                .lancamentos_vivos_da_origem(ctx.empresa, "financeiro", "saldo_inicial", self.conta)
                .map_err(|e| Erro::de_dominio(&e))?;
            for anterior in vivos {
                cardeal_ledger::Razao::estornar(
                    &mut repo,
                    anterior,
                    "Saldo inicial redefinido",
                    ctx.hoje(),
                )
                .map_err(|e| Erro::de_dominio(&e))?;
            }
        }

        let lancamento = if self.saldo.e_zero() {
            None
        } else {
            let mut repo = cardeal_ledger::RepositorioRazao::novo(uow);
            let lucros = cardeal_ledger::Contas::nova(&repo, ctx.empresa)
                .papel(cardeal_ledger::PapelConta::LucrosAcumulados)
                .map_err(|e| Erro::de_dominio(&e))?;
            let valor = self.saldo.abs();
            let (debito, credito) = if self.saldo.e_positivo() {
                (self.conta, lucros)
            } else {
                (lucros, self.conta)
            };
            let lanc = cardeal_ledger::ConstrutorLancamento::novo(
                ctx.empresa,
                self.data,
                format!("Saldo inicial de {}", conta.nome),
            )
            .origem(
                cardeal_ledger::Origem::modulo("financeiro")
                    .tipo("saldo_inicial")
                    .agregado(self.conta),
            )
            .liquidacao(self.data)
            .criado_por(ctx.usuario, ctx.dispositivo)
            .agora(ctx.agora)
            .debitar(debito, valor)
            .creditar(credito, valor)
            .construir()
            .map_err(|e| Erro::de_dominio(&e))?;
            Some(
                cardeal_ledger::Razao::registrar(&mut repo, lanc)
                    .map_err(|e| Erro::de_dominio(&e))?,
            )
        };
        Ok(SaldoInicialDefinido {
            lancamento,
            saldo_atual: saldo(uow, self.conta)?,
        })
    }
}

// ── transferência ────────────────────────────────────────────────────────────

/// Transfere entre duas contas do Disponível (depositar o caixa no banco, mover de um banco
/// para outro).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferirEntreContas {
    /// De onde sai.
    pub origem: Id,
    /// Para onde vai.
    pub destino: Id,
    /// Quanto.
    pub valor: Dinheiro,
    /// Quando.
    pub data: Data,
    /// Observação livre.
    pub observacao: Option<String>,
}

/// O que a transferência devolve.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransferenciaFeita {
    /// A transferência.
    pub transferencia: Id,
    /// O lançamento `Realizado` (D destino / C origem).
    pub lancamento: Id,
}

impl Comando for TransferirEntreContas {
    type Saida = TransferenciaFeita;
    const PERMISSAO: &'static str = "financeiro.banco.transferir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        if !self.valor.e_positivo() {
            return Err(Erro::de_dominio(&ErroFinanceiro::ValorInvalido));
        }
        if self.origem == self.destino {
            return Err(Erro::de_dominio(
                &ErroFinanceiro::TransferenciaParaAMesmaConta,
            ));
        }
        let origem = conta_disponivel(uow.conexao(), ctx.empresa, self.origem)?;
        let destino = conta_disponivel(uow.conexao(), ctx.empresa, self.destino)?;
        if e_caixa(
            uow.conexao(),
            ctx.empresa,
            self.origem,
            origem.papel.as_deref(),
        )? {
            let disponivel = saldo(uow, self.origem)?;
            if disponivel < self.valor {
                return Err(Erro::de_dominio(
                    &ErroFinanceiro::SaldoInsuficienteNaOrigem { disponivel },
                ));
            }
        }

        let id = Id::novo();
        let lanc = cardeal_ledger::ConstrutorLancamento::novo(
            ctx.empresa,
            self.data,
            format!("Transferência de {} para {}", origem.nome, destino.nome),
        )
        .origem(
            cardeal_ledger::Origem::modulo("financeiro")
                .tipo("transferencia")
                .agregado(id),
        )
        .liquidacao(self.data)
        .criado_por(ctx.usuario, ctx.dispositivo)
        .agora(ctx.agora)
        .debitar(self.destino, self.valor)
        .creditar(self.origem, self.valor)
        .construir()
        .map_err(|e| Erro::de_dominio(&e))?;
        let lancamento = cardeal_ledger::Razao::registrar(
            &mut cardeal_ledger::RepositorioRazao::novo(uow),
            lanc,
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        uow.conexao()
            .execute(
                "INSERT INTO financeiro_transferencia
                   (id, empresa, origem, destino, valor, data, observacao, lancamento,
                    criado_em, criado_por)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    blob(id),
                    blob(ctx.empresa),
                    blob(self.origem),
                    blob(self.destino),
                    self.valor.em_centavos(),
                    i64::from(self.data.em_dias()),
                    self.observacao
                        .as_deref()
                        .map(str::trim)
                        .filter(|o| !o.is_empty()),
                    blob(lancamento),
                    ctx.agora.em_micros(),
                    blob(ctx.usuario),
                ],
            )
            .map_err(persist)?;
        Ok(TransferenciaFeita {
            transferencia: id,
            lancamento,
        })
    }
}

/// Cancela uma transferência: estorna o lançamento (os dois lados de uma vez).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarTransferencia {
    /// A transferência.
    pub transferencia: Id,
    /// Por que (ao menos 10 caracteres; vai ao histórico do estorno).
    pub motivo: String,
}

impl Comando for CancelarTransferencia {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.banco.transferir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        let motivo = self.motivo.trim().to_owned();
        if motivo.chars().count() < 10 {
            return Err(Erro::de_dominio(&ErroFinanceiro::MotivoDoCancelamentoCurto));
        }
        let linha: Option<(Vec<u8>, Option<i64>)> = uow
            .conexao()
            .query_row(
                "SELECT lancamento, cancelada_em FROM financeiro_transferencia
                 WHERE empresa = ?1 AND id = ?2",
                params![blob(ctx.empresa), blob(self.transferencia)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(persist)?;
        let Some((lancamento, cancelada)) = linha else {
            return Err(Erro::nao_encontrado("transferência"));
        };
        if cancelada.is_some() {
            return Err(Erro::de_dominio(&ErroFinanceiro::TransferenciaJaCancelada));
        }
        cardeal_ledger::Razao::estornar(
            &mut cardeal_ledger::RepositorioRazao::novo(uow),
            id_de(lancamento),
            &format!("Transferência cancelada — {motivo}"),
            ctx.hoje(),
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        uow.conexao()
            .execute(
                "UPDATE financeiro_transferencia SET cancelada_em = ?3, motivo_cancelamento = ?4
                 WHERE empresa = ?1 AND id = ?2",
                params![
                    blob(ctx.empresa),
                    blob(self.transferencia),
                    ctx.agora.em_micros(),
                    motivo
                ],
            )
            .map_err(persist)?;
        Ok(())
    }
}

/// As transferências, as mais novas primeiro (até 200).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transferencias;

/// Uma transferência da lista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemTransferencia {
    /// A transferência.
    pub id: Id,
    /// A conta de origem (nome).
    pub origem: String,
    /// A conta de destino (nome).
    pub destino: String,
    /// O valor.
    pub valor: Dinheiro,
    /// A data.
    pub data: Data,
    /// A observação.
    pub observacao: Option<String>,
    /// Quando foi cancelada, se foi.
    pub cancelada_em: Option<Instante>,
}

impl Consulta for Transferencias {
    type Saida = Vec<ItemTransferencia>;
    const PERMISSAO: &'static str = "financeiro.banco.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = c
            .prepare(
                "SELECT t.id, o.nome, d.nome, t.valor, t.data, t.observacao, t.cancelada_em
                 FROM financeiro_transferencia t
                 JOIN razao_conta o ON o.id = t.origem
                 JOIN razao_conta d ON d.id = t.destino
                 WHERE t.empresa = ?1
                 ORDER BY t.data DESC, t.criado_em DESC
                 LIMIT 200",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemTransferencia {
                    id: id_de(r.get(0)?),
                    origem: r.get(1)?,
                    destino: r.get(2)?,
                    valor: Dinheiro::centavos(r.get(3)?),
                    data: data_de(r.get(4)?),
                    observacao: r.get(5)?,
                    cancelada_em: r.get::<_, Option<i64>>(6)?.map(Instante::de_micros),
                })
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist);
        linhas
    }
}

// ── desativar ────────────────────────────────────────────────────────────────

/// Desativa uma conta do Disponível que não se usa mais. Recusa as do sistema (Caixa, Bancos,
/// Aplicações, Valores em trânsito), a conta de um caixa cadastrado e a que ainda tem saldo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DesativarContaDisponivel {
    /// A conta.
    pub conta: Id,
}

impl Comando for DesativarContaDisponivel {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.banco.editar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        let conta = conta_disponivel(uow.conexao(), ctx.empresa, self.conta)?;
        if conta
            .papel
            .as_deref()
            .is_some_and(|p| PAPEIS_DO_SISTEMA.contains(&p))
        {
            return Err(Erro::de_dominio(&ErroFinanceiro::ContaDisponivelInvalida(
                "Esta é uma conta do sistema (usada pelo PDV, pela sangria e pelas baixas): não se desativa",
            )));
        }
        if e_caixa(uow.conexao(), ctx.empresa, self.conta, None)? {
            return Err(Erro::de_dominio(&ErroFinanceiro::ContaDisponivelInvalida(
                "Esta conta é de um caixa cadastrado: desative o caixa antes",
            )));
        }
        let atual = saldo(uow, self.conta)?;
        if !atual.e_zero() {
            return Err(Erro::de_dominio(&ErroFinanceiro::ContaComSaldo(atual)));
        }
        cardeal_ledger::RepositorioRazao::novo(uow)
            .desativar_conta(self.conta)
            .map_err(|e| Erro::de_dominio(&e))
    }
}
