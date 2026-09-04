//! Lança um título a receber avulso — cria o [`Titulo`](crate::Titulo), as parcelas e um
//! lançamento `Confirmado` por parcela.
//!
//! Receituário (`docs/modulos/financeiro.md` §7): D Clientes a receber · C Receita
//! (a obrigação existe; o dinheiro ainda não andou).

use cardeal_kernel::{Data, Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{Contas, Contraparte, PapelConta, Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::autoria_de;
use crate::eventos::TituloLancado;
use crate::receituario::{lancar_titulo, ContasTitulo};
use crate::repositorio::RepositorioFinanceiro;
use crate::titulo::{ConstrutorTitulo, EspecieTitulo};

/// Lança um título a receber com uma ou mais parcelas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LancarTituloAReceber {
    /// O cliente devedor.
    pub cliente: Id,
    /// O valor total (soma das parcelas).
    pub valor_total: Dinheiro,
    /// A data de emissão.
    pub emissao: Data,
    /// Quantas parcelas (1..=360).
    pub parcelas: u16,
    /// O vencimento da primeira parcela.
    pub primeiro_vencimento: Data,
    /// Dias entre parcelas consecutivas.
    pub intervalo_dias: i32,
    /// Observação livre.
    pub observacao: Option<String>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct TituloAReceberLancado {
    /// O título criado.
    pub titulo: Id,
    /// As parcelas, em ordem.
    pub parcelas: Vec<Id>,
    /// Os lançamentos `Confirmado` gerados, um por parcela.
    pub lancamentos: Vec<Id>,
}

impl Comando for LancarTituloAReceber {
    type Saida = TituloAReceberLancado;
    const PERMISSAO: &'static str = "financeiro.receber.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar (domínio puro): monta título + parcelas com rateio que fecha ao centavo.
        let mut construtor = ConstrutorTitulo::novo(
            ctx.empresa,
            EspecieTitulo::Receber,
            Contraparte::Cliente(self.cliente),
            self.valor_total,
            self.emissao,
        )
        .parcelas(self.parcelas, self.primeiro_vencimento, self.intervalo_dias);
        if let Some(obs) = self.observacao {
            construtor = construtor.observacao(obs);
        }
        let mut tcp = construtor.construir().map_err(|e| Erro::de_dominio(&e))?;

        // 2. Resolver contas por papel.
        let (conta_cliente, conta_receita) = {
            let repo = RepositorioRazao::novo(uow);
            let contas = Contas::nova(&repo, ctx.empresa);
            (
                contas
                    .papel(PapelConta::ClientesAReceber)
                    .map_err(|e| Erro::de_dominio(&e))?,
                contas
                    .papel(PapelConta::ReceitaVendas)
                    .map_err(|e| Erro::de_dominio(&e))?,
            )
        };

        // 3. Receituário + Razão: um lançamento por parcela, vinculado a ela.
        let lancs = lancar_titulo(
            &tcp,
            ContasTitulo {
                contraparte: conta_cliente,
                resultado: conta_receita,
            },
            autoria_de(ctx),
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        let mut lancamentos = Vec::with_capacity(lancs.len());
        {
            let mut repo = RepositorioRazao::novo(uow);
            for (parcela, lanc) in tcp.parcelas.iter_mut().zip(lancs) {
                let id = Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?;
                parcela.lancamento = Some(id);
                lancamentos.push(id);
            }
        }

        // 4. Persistir.
        RepositorioFinanceiro::novo(uow).inserir_titulo(&tcp)?;

        // 5. Publicar.
        let parcelas: Vec<Id> = tcp.parcelas.iter().map(|p| p.id).collect();
        let quantas = u16::try_from(parcelas.len()).unwrap_or(u16::MAX);
        uow.publicar(TituloLancado {
            titulo: tcp.titulo.id,
            especie: "Receber",
            valor: tcp.titulo.valor_original,
            parcelas: quantas,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(TituloAReceberLancado {
            titulo: tcp.titulo.id,
            parcelas,
            lancamentos,
        })
    }
}
