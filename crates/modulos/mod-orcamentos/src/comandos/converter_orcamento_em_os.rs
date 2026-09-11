//! Converte um orçamento aprovado numa ordem de serviço.
//!
//! Chama [`mod_os`] **direto, na mesma transação** — mesmo padrão de
//! `mod_os::FaturarOrdemServico` chamando `mod_financeiro` (`docs/contratos-internos.md` §7
//! regra 2): atomicidade importa mais que desacoplamento a qualquer custo. A dependência é
//! só num sentido — `mod-os` não conhece `mod-orcamentos`.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_os::{ItemMaoDeObra, OrdemServico, RepositorioOs};
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_orcamento;
use crate::erros::ErroOrcamentos;
use crate::eventos::OrcamentoConvertidoEmOs;
use crate::repositorio::RepositorioOrcamentos;

/// Converte um orçamento aprovado em OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConverterOrcamentoEmOs {
    /// O orçamento aprovado.
    pub orcamento: Id,
    /// O técnico responsável pela OS.
    pub tecnico_responsavel: Id,
    /// Prazo de garantia da OS, em dias.
    pub garantia_dias: u16,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrcamentoConvertido {
    /// A ordem de serviço criada.
    pub ordem_servico: Id,
    /// O número da OS.
    pub numero: u64,
}

impl Comando for ConverterOrcamentoEmOs {
    type Saida = OrcamentoConvertido;
    const PERMISSAO: &'static str = "orcamentos.orcamento.converter";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        if !ctx.ativo("os") {
            return Err(Erro::de_dominio(&ErroOrcamentos::ModuloOsInativo));
        }

        let mut orcamento = carregar_orcamento(uow, self.orcamento)?;
        let Some(cliente) = orcamento.cliente else {
            return Err(Erro::de_dominio(&ErroOrcamentos::ClienteAvulsoNaoConverte));
        };
        let itens = RepositorioOrcamentos::novo(uow).itens_do_orcamento(orcamento.id)?;

        // ── Abre a OS ───────────────────────────────────────────────────────
        // `defeito_relatado` (obrigatório em `mod_os`) vem da descrição/escopo do orçamento
        // quando houver; sem ela, o próprio assunto serve — nunca fica vazio.
        let defeito_relatado = orcamento
            .descricao
            .clone()
            .unwrap_or_else(|| orcamento.assunto.clone());
        let numero_os = RepositorioOs::novo(uow).proximo_numero()?;
        let mut os = OrdemServico::abrir(
            ctx.empresa,
            numero_os,
            cliente,
            orcamento.assunto.clone(),
            defeito_relatado,
            self.tecnico_responsavel,
            ctx.hoje(),
            self.garantia_dias,
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOs::novo(uow).inserir_ordem(&os)?;

        // ── Copia os itens como mão de obra ─────────────────────────────────
        for item in &itens {
            let mob =
                ItemMaoDeObra::novo(os.id, item.descricao.clone(), item.total, ctx.usuario, None)
                    .map_err(|e| Erro::de_dominio(&e))?;
            RepositorioOs::novo(uow).inserir_item_mao_de_obra(&mob)?;
            os.adicionar_ao_orcamento(item.total)
                .map_err(|e| Erro::de_dominio(&e))?;
        }
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        // ── Marca o orçamento e publica ────────────────────────────────────
        orcamento
            .marcar_convertido(os.id)
            .map_err(|e| Erro::de_dominio(&e))?;
        RepositorioOrcamentos::novo(uow).atualizar_orcamento(&orcamento)?;

        uow.publicar(OrcamentoConvertidoEmOs {
            orcamento: orcamento.id,
            ordem_servico: os.id,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(OrcamentoConvertido {
            ordem_servico: os.id,
            numero: numero_os,
        })
    }
}
