//! Renegocia o saldo em aberto de um título: fecha as parcelas ainda não quitadas e abre um
//! `Titulo` novo com as condições novas.
//!
//! `docs/modulos/financeiro.md` §5. **Decisão de contabilização**: a receita/despesa já foi
//! reconhecida quando o título original foi lançado (`LancarTitulo`, um `Confirmado` por
//! parcela) — renegociar só reagenda o recebimento/pagamento, não é um fato gerador novo. Por
//! isso este comando **não** cria lançamento nenhum no Razão: as parcelas novas nascem com
//! `lancamento: None` (o mesmo estado transitório que `ConstrutorTitulo::construir` já produz
//! antes de `LancarTitulo` postar), e as antigas mantêm intacto o lançamento que já as
//! representava. `ErroFinanceiro::SemSaldoParaRenegociar` cobre tanto "título já quitado"
//! quanto o `TituloQuitado` que o spec original citava — é o mesmo caso.

#![allow(clippy::too_many_arguments)]

use cardeal_kernel::{Data, Dinheiro, Erro, Id, Percentual, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::erros::ErroFinanceiro;
use crate::eventos::TituloRenegociado;
use crate::repositorio::RepositorioFinanceiro;
use crate::titulo::{ConstrutorTitulo, EstadoParcela, PoliticaJuros};

/// Renegocia o saldo em aberto de um título.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenegociarTitulo {
    /// O título a renegociar.
    pub titulo: Id,
    /// Quantidade de parcelas do novo título.
    pub numero_parcelas: u16,
    /// Vencimento da 1ª parcela nova.
    pub primeiro_vencimento: Data,
    /// Intervalo em dias entre as parcelas novas.
    pub intervalo_dias: i32,
    /// Política de juros de mora das parcelas novas.
    pub politica_juros: PoliticaJuros,
    /// A taxa, obrigatória se `politica_juros` não for [`PoliticaJuros::Nenhum`].
    pub taxa_juros: Option<Percentual>,
    /// Multa por atraso das parcelas novas.
    pub multa: Option<Percentual>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct TituloFoiRenegociado {
    /// O novo título, com as condições novas.
    pub titulo_novo: Id,
    /// O saldo consolidado nas novas condições.
    pub saldo: Dinheiro,
}

impl Comando for RenegociarTitulo {
    type Saida = TituloFoiRenegociado;
    const PERMISSAO: &'static str = "financeiro.receber.renegociar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let titulo_original = RepositorioFinanceiro::novo(uow)
            .buscar_titulo(self.titulo)?
            .ok_or_else(|| Erro::nao_encontrado("título"))?;
        let parcelas = RepositorioFinanceiro::novo(uow).parcelas_do_titulo(self.titulo)?;
        let abertas: Vec<_> = parcelas
            .into_iter()
            .filter(|p| p.estado.aceita_baixa())
            .collect();

        // 2. Validar (domínio puro): precisa de saldo para renegociar.
        let saldo: Dinheiro = abertas
            .iter()
            .map(crate::titulo::Parcela::saldo_principal)
            .sum();
        if !saldo.e_positivo() {
            return Err(Erro::de_dominio(&ErroFinanceiro::SemSaldoParaRenegociar));
        }

        let mut construtor = ConstrutorTitulo::novo(
            ctx.empresa,
            titulo_original.especie,
            titulo_original.contraparte,
            saldo,
            ctx.hoje(),
        )
        .origem("financeiro", Some(titulo_original.id))
        .parcelas(
            self.numero_parcelas,
            self.primeiro_vencimento,
            self.intervalo_dias,
        );
        if self.politica_juros.exige_taxa() {
            construtor = construtor.juros(
                self.politica_juros,
                self.taxa_juros.unwrap_or(Percentual::ZERO),
            );
        }
        if let Some(multa) = self.multa {
            construtor = construtor.multa(multa);
        }
        let tcp_novo = construtor.construir().map_err(|e| Erro::de_dominio(&e))?;
        let titulo_novo = tcp_novo.titulo.id;

        // 4. Persistir: o título novo, e as parcelas antigas absorvidas.
        {
            let mut repo = RepositorioFinanceiro::novo(uow);
            repo.inserir_titulo(&tcp_novo)?;
            for mut p in abertas {
                p.estado = EstadoParcela::Renegociada;
                p.versao = p.versao.proxima();
                repo.atualizar_parcela(&p)?;
            }
        }

        // 5. Publicar.
        uow.publicar(TituloRenegociado {
            titulo_original: titulo_original.id,
            titulo_novo,
            saldo,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(TituloFoiRenegociado { titulo_novo, saldo })
    }
}
