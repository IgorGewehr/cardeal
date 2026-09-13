//! Cancela a ordem de serviço antes de faturada: `Aberta`/`EmDiagnostico`/
//! `AguardandoAprovacao`/`Aprovada`/`EmExecucao` → `Cancelada` (terminal).
//! `docs/modulos/os.md` §4 e §11 regra 5.
//!
//! Quando a OS já tem peça aplicada (só possível a partir de `EmExecucao`), estorna cada
//! [`crate::execucao::ItemPeca`] aplicado e ainda não estornado de volta ao estoque — chama
//! `mod_estoque::registrar_entrada_comum` **direto, na mesma transação**, mesma decisão de
//! arquitetura de [`super::aplicar_peca`]. Um item aplicado antes desta rastreabilidade
//! existir (sem `local` gravado) não pode ser devolvido automaticamente — entra em
//! `pecas_pendentes_de_estorno_manual` no retorno, para o operador corrigir o estoque à mão;
//! nunca fica um consumo órfão silencioso.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::{registrar_entrada_comum, DadosEntrada};
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::eventos::OrdemCancelada;
use crate::repositorio::RepositorioOs;

/// Cancela a ordem de serviço.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CancelarOrdemServico {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrdemServicoCancelada {
    /// Quantas peças aplicadas foram estornadas (devolvidas ao estoque) automaticamente.
    pub pecas_estornadas: usize,
    /// Quantas peças aplicadas **não puderam** ser estornadas automaticamente (aplicadas
    /// antes de o local ser gravado) — precisam de correção manual de estoque.
    pub pecas_pendentes_de_estorno_manual: usize,
}

impl Comando for CancelarOrdemServico {
    type Saida = OrdemServicoCancelada;
    const PERMISSAO: &'static str = "os.ordem.cancelar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;

        // 2. Validar (domínio puro): transiciona para Cancelada a partir de qualquer estado
        // não-terminal anterior a Concluida/Faturada.
        os.cancelar().map_err(|e| Erro::de_dominio(&e))?;

        // 3. Estornar peças já aplicadas (só possível se a OS chegou a `EmExecucao`) — devolve
        // cada uma ao local de onde saiu, pelo custo com que foi aplicada.
        let itens_peca = RepositorioOs::novo(uow).itens_peca_da_ordem(os.id)?;
        let mut pecas_estornadas = 0usize;
        let mut pecas_pendentes_de_estorno_manual = 0usize;
        for mut item in itens_peca {
            if !item.aplicada || item.estornada {
                continue;
            }
            let Some(local) = item.local else {
                // Aplicada antes desta migração gravar o local — não há para onde devolver
                // automaticamente; sinaliza para correção manual.
                pecas_pendentes_de_estorno_manual += 1;
                continue;
            };
            registrar_entrada_comum(
                DadosEntrada {
                    produto: item.produto,
                    local,
                    quantidade: item.quantidade,
                    custo_unitario: item.custo_unitario,
                    origem_modulo: "os",
                    origem_id: Some(os.id),
                },
                ctx,
                uow,
            )?;
            item.estornar().map_err(|e| Erro::de_dominio(&e))?;
            RepositorioOs::novo(uow).atualizar_item_peca(&item)?;
            pecas_estornadas += 1;
        }

        // 4. Persistir.
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        // 5. Publicar.
        uow.publicar(OrdemCancelada {
            ordem_servico: os.id,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(OrdemServicoCancelada {
            pecas_estornadas,
            pecas_pendentes_de_estorno_manual,
        })
    }
}
