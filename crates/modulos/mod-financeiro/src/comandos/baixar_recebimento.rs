//! Dá baixa (total ou parcial) numa parcela de título **a receber**.
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Caixa/Bancos (valor recebido) +
//! D Descontos concedidos (desconto) · C Clientes a receber (principal) +
//! C Receita financeira (juros + multa). Juros, multa e desconto são calculados **na data
//! informada**, nunca materializados antes (`docs/modulos/financeiro.md` §11.1).

use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{Contas, PapelConta, Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::autoria_de;
use crate::eventos::ParcelaBaixada;
use crate::receituario::{baixar_parcela, ContasBaixa};
use crate::repositorio::{BaixaGravada, RepositorioFinanceiro};
use crate::titulo::EspecieTitulo;

/// Baixa uma parcela a receber.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaixarRecebimento {
    /// A parcela a baixar.
    pub parcela: Id,
    /// O valor recebido (principal + encargos − desconto, quando total).
    pub valor: Dinheiro,
    /// A data do recebimento — define juros, multa e disponibilidade de desconto.
    pub data: Data,
    /// A conta que recebeu o dinheiro. `None` = a conta de Caixa da empresa.
    pub conta_destino: Option<Id>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct RecebimentoBaixado {
    /// A baixa criada.
    pub baixa: Id,
    /// O lançamento `Realizado` gerado.
    pub lancamento: Id,
    /// Verdadeiro se a baixa quitou a parcela.
    pub parcela_quitada: bool,
    /// O saldo de principal que resta na parcela.
    pub saldo_restante: Dinheiro,
}

impl Comando for BaixarRecebimento {
    type Saida = RecebimentoBaixado;
    const PERMISSAO: &'static str = "financeiro.receber.baixar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut parcela = RepositorioFinanceiro::novo(uow)
            .buscar_parcela(self.parcela)?
            .ok_or_else(|| Erro::nao_encontrado("parcela"))?;
        let titulo = RepositorioFinanceiro::novo(uow)
            .buscar_titulo(parcela.titulo)?
            .ok_or_else(|| Erro::nao_encontrado("título"))?;

        if titulo.especie != EspecieTitulo::Receber {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "esta parcela pertence a um título a pagar — use a baixa de pagamento",
            ));
        }

        // 2. Validar (domínio puro): juros/multa/desconto na data, e o rateio da baixa.
        let plano = parcela
            .planejar_baixa(self.data, self.valor)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 3. Resolver contas (só as que o plano realmente move) e montar o lançamento.
        let contas_baixa = {
            let repo = RepositorioRazao::novo(uow);
            let contas = Contas::nova(&repo, ctx.empresa);
            let destino = match self.conta_destino {
                Some(id) => id,
                None => contas
                    .papel(PapelConta::Caixa)
                    .map_err(|e| Erro::de_dominio(&e))?,
            };
            let encargos = if (plano.juros + plano.multa).e_positivo() {
                contas
                    .papel(PapelConta::ReceitaFinanceira)
                    .map_err(|e| Erro::de_dominio(&e))?
            } else {
                Id::NULO
            };
            let desconto = if plano.desconto.e_positivo() {
                contas
                    .papel(PapelConta::DescontosConcedidos)
                    .map_err(|e| Erro::de_dominio(&e))?
            } else {
                Id::NULO
            };
            ContasBaixa {
                conta_destino: destino,
                contraparte: contas
                    .papel(PapelConta::ClientesAReceber)
                    .map_err(|e| Erro::de_dominio(&e))?,
                encargos,
                desconto,
            }
        };

        let lanc = baixar_parcela(
            EspecieTitulo::Receber,
            titulo.contraparte,
            &parcela,
            &plano,
            contas_baixa,
            autoria_de(ctx),
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        let lancamento = {
            let mut repo = RepositorioRazao::novo(uow);
            Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
        };

        // 4. Persistir: avança a parcela e grava a baixa.
        parcela.aplicar_baixa(&plano);
        let baixa = BaixaGravada {
            id: Id::novo(),
            empresa: ctx.empresa,
            parcela: parcela.id,
            data: plano.data,
            valor_recebido: plano.valor_recebido,
            principal: plano.principal,
            juros: plano.juros,
            multa: plano.multa,
            desconto: plano.desconto,
            lancamento,
        };
        {
            let mut repo = RepositorioFinanceiro::novo(uow);
            repo.atualizar_parcela(&parcela)?;
            repo.inserir_baixa(&baixa)?;
        }

        // 5. Publicar.
        uow.publicar(ParcelaBaixada {
            parcela: parcela.id,
            titulo: titulo.id,
            valor_recebido: plano.valor_recebido,
            quitada: plano.quita,
            lancamento,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(RecebimentoBaixado {
            baixa: baixa.id,
            lancamento,
            parcela_quitada: plano.quita,
            saldo_restante: parcela.saldo_principal(),
        })
    }
}
