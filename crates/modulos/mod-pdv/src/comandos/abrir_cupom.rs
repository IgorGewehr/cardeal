//! Abre um cupom `EmAndamento`, sem itens. `docs/modulos/pdv.md` §5.
//!
//! **Nota de desenho**: o spec original não lista `AbrirCupom` como comando próprio —
//! `AdicionarItem` "cria `Cupom` se não existir" implicitamente. Esta fatia segue o padrão do
//! resto do código (`CriarPedido`, `AbrirOrdemServico`, `LancarTitulo` — sempre um comando
//! explícito de abertura) para não misturar duas responsabilidades num só `Comando`.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_financeiro::{EstadoSessao, RepositorioFinanceiro};
use serde::{Deserialize, Serialize};

use crate::cupom::Cupom;
use crate::repositorio::RepositorioPdv;

/// Abre um cupom novo no terminal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AbrirCupom {
    /// A sessão de caixa — precisa estar `Aberta`.
    pub sessao_caixa: Id,
    /// O terminal.
    pub terminal: Id,
    /// A série fiscal do terminal.
    pub serie_fiscal: i64,
    /// O cliente identificado, se houver.
    pub cliente: Option<Id>,
    /// A tabela de preço a usar na resolução de preço dos itens.
    pub tabela_preco: Id,
    /// O local de estoque de onde os itens saem ao finalizar.
    pub local_expedicao: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct CupomAberto {
    /// O cupom criado.
    pub cupom: Id,
    /// O número dentro da faixa do terminal.
    pub numero_terminal: i64,
}

impl Comando for AbrirCupom {
    type Saida = CupomAberto;
    const PERMISSAO: &'static str = "pdv.venda.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar: a sessão de caixa precisa estar aberta.
        let sessao = RepositorioFinanceiro::novo(uow)
            .buscar_sessao(self.sessao_caixa)?
            .ok_or_else(|| Erro::nao_encontrado("sessão de caixa"))?;
        if sessao.estado != EstadoSessao::Aberta {
            return Err(Erro::novo(
                cardeal_kernel::CodigoErro::CAIXA_FECHADO,
                "a sessão de caixa deste terminal não está aberta",
            ));
        }

        // 2. Validar (domínio puro): monta o cupom com o próximo número da faixa.
        let numero_terminal = RepositorioPdv::novo(uow).proximo_numero_cupom(
            ctx.empresa,
            self.terminal,
            self.serie_fiscal,
        )?;
        let cupom = Cupom::novo(
            ctx.empresa,
            self.sessao_caixa,
            self.terminal,
            numero_terminal,
            self.serie_fiscal,
            self.cliente,
            ctx.usuario,
            ctx.agora,
            self.tabela_preco,
            self.local_expedicao,
        );

        // 4. Persistir.
        RepositorioPdv::novo(uow).inserir_cupom(&cupom)?;

        Ok(CupomAberto {
            cupom: cupom.id,
            numero_terminal,
        })
    }
}
